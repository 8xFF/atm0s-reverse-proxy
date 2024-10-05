//! Task runner for a single connection
//! This must ensure not blocking by other actor.
//! We have some strict rules
//!
//! - Only use async with current connection stream
//! - For other communication shoud use try_send for avoding blocking

use anyhow::anyhow;
use futures::{SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
use tokio::{
    io::copy_bidirectional,
    select,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::codec::Framed;

use crate::{
    ctx::SharedCtx,
    msg::{PeerMessage, ServiceId, StreamConnectReq, StreamConnectRes},
    router::RouteAction,
    stream::{wait_object, write_object, BincodeCodec, P2pQuicStream},
    utils::ErrorExt,
    InternalEvent, P2pServiceEvent, PeerAddress, PeerMainData,
};

use super::PeerConnectionControl;

pub struct PeerConnectionInternal {
    ctx: SharedCtx,
    remote: PeerAddress,
    connection: Connection,
    framed: Framed<P2pQuicStream, BincodeCodec<PeerMessage>>,
    internal_tx: Sender<InternalEvent>,
    control_rx: Receiver<PeerConnectionControl>,
}

impl PeerConnectionInternal {
    pub fn new(ctx: SharedCtx, connection: Connection, main_send: SendStream, main_recv: RecvStream, internal_tx: Sender<InternalEvent>, control_rx: Receiver<PeerConnectionControl>) -> Self {
        let stream = P2pQuicStream::new(main_recv, main_send);

        Self {
            ctx,
            remote: connection.remote_address().into(),
            connection,
            framed: Framed::new(stream, BincodeCodec::default()),
            internal_tx,
            control_rx,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        Ok(self.framed.send(PeerMessage::Hello {}).await?)
    }

    pub async fn recv_complex(&mut self) -> anyhow::Result<()> {
        select! {
            open = self.connection.accept_bi() => {
                let (send, recv) = open?;
                self.on_accept_bi(send, recv).await?;
                Ok(())
            },
            frame = self.framed.next() => {
                let msg = frame.ok_or(anyhow!("peer main stream ended"))??;
                self.on_msg(msg).await
            },
            control = self.control_rx.recv() => {
                let control = control.ok_or(anyhow!("peer control channel ended"))?;
                self.on_control(control).await
            }
        }
    }

    async fn on_accept_bi(&mut self, send: SendStream, recv: RecvStream) -> anyhow::Result<()> {
        log::info!("[PeerConnectionInternal {}] on new bi", self.remote);
        let stream = P2pQuicStream::new(recv, send);
        tokio::spawn(accept_bi(self.remote, stream, self.ctx.clone()));
        Ok(())
    }

    async fn on_control(&mut self, control: PeerConnectionControl) -> anyhow::Result<()> {
        match control {
            PeerConnectionControl::Send(item) => Ok(self.framed.send(item).await?),
            PeerConnectionControl::OpenStream(service, source, dest, meta, tx) => {
                let remote = self.remote;
                let connection = self.connection.clone();
                tokio::spawn(async move {
                    log::info!("[PeerConnectionInternal {remote}] open_bi for service {service}");
                    let res = open_bi(connection, source, dest, service, meta).await;
                    if let Err(e) = &res {
                        log::error!("[PeerConnectionInternal {remote}] open_bi for service {service} error {e}");
                    } else {
                        log::info!("[PeerConnectionInternal {remote}] open_bi for service {service} success");
                    }
                    tx.send(res).map_err(|_| "internal channel error").print_on_err("[PeerConnectionInternal] answer open_bi");
                });
                Ok(())
            }
        }
    }

    async fn on_msg(&mut self, msg: PeerMessage) -> anyhow::Result<()> {
        match msg {
            PeerMessage::Hello {} => {
                log::info!("[PeerConnectionInternal {}] on hello", self.remote);
            }
            PeerMessage::Sync { route, advertise } => {
                if let Err(_e) = self.internal_tx.try_send(InternalEvent::PeerData(self.remote, PeerMainData::Sync { route, advertise })) {
                    log::warn!("[PeerConnectionInternal {}] queue main loop full", self.remote);
                }
            }
            PeerMessage::Broadcast(source, service_id, msg_id, data) => {
                if self.ctx.check_broadcast_msg(msg_id) {
                    for peer in self.ctx.peers().into_iter().filter(|p| !self.remote.eq(&p.address())) {
                        peer.try_send(PeerMessage::Broadcast(source, service_id, msg_id, data.clone()))
                            .print_on_err("[PeerConnectionInternal] broadcast data over peer alias");
                    }

                    if let Some(service) = self.ctx.service(&service_id) {
                        service.try_send(P2pServiceEvent::Broadcast(source, data)).print_on_err("[PeerConnectionInternal] send service msg");
                    }
                } else {
                    log::debug!("[PeerConnectionInternal {}] broadcast msg {msg_id} already deliveried", self.remote);
                }
            }
            PeerMessage::Unicast(source, dest, service_id, data) => match self.ctx.router().action(&dest) {
                Some(RouteAction::Local) => {
                    if let Some(service) = self.ctx.service(&service_id) {
                        service.try_send(P2pServiceEvent::Unicast(source, data)).print_on_err("[PeerConnectionInternal] send service msg");
                    } else {
                        log::warn!("[PeerConnectionInternal {}] service {service_id} not found", self.remote);
                    }
                }
                Some(RouteAction::Next(next)) => {
                    if let Some(peer) = self.ctx.peer(&next) {
                        peer.try_send(PeerMessage::Unicast(source, dest, service_id, data))
                            .print_on_err("[PeerConnectionInternal] send data over peer alias");
                    } else {
                        log::warn!("[PeerConnectionInternal {}] peer {next} not found", self.remote);
                    }
                }
                None => {
                    log::warn!("[PeerConnectionInternal {}] path to {dest} not found", self.remote);
                }
            },
        }
        Ok(())
    }
}

async fn open_bi(connection: Connection, source: PeerAddress, dest: PeerAddress, service: ServiceId, meta: Vec<u8>) -> anyhow::Result<P2pQuicStream> {
    let (send, recv) = connection.open_bi().await?;
    let mut stream = P2pQuicStream::new(recv, send);
    write_object::<_, _, 500>(&mut stream, &StreamConnectReq { source, dest, service, meta }).await?;
    let res = wait_object::<_, StreamConnectRes, 500>(&mut stream).await?;
    res.map(|_| stream).map_err(|e| anyhow!("{e}"))
}

async fn accept_bi(remote: PeerAddress, mut stream: P2pQuicStream, ctx: SharedCtx) -> anyhow::Result<()> {
    let req = wait_object::<_, StreamConnectReq, 500>(&mut stream).await?;
    let StreamConnectReq { dest, source, service, meta } = req;
    match ctx.router().action(&dest) {
        Some(RouteAction::Local) => {
            if let Some(service_tx) = ctx.service(&service) {
                log::info!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} => process local");
                write_object::<_, _, 500>(&mut stream, &Ok::<_, String>(())).await?;
                service_tx
                    .send(P2pServiceEvent::Stream(source, meta, stream))
                    .await
                    .print_on_err("[PeerConnectionInternal] send accpeted stream to service");
                Ok(())
            } else {
                log::warn!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} => service not found");
                write_object::<_, _, 500>(&mut stream, &Err::<(), _>("service not found".to_string())).await?;
                Err(anyhow!("service not found"))
            }
        }
        Some(RouteAction::Next(next)) => {
            if let Some(alias) = ctx.peer(&next) {
                log::info!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} => forward to {next}");
                match alias.open_stream(service, source, dest, meta).await {
                    Ok(mut next_stream) => {
                        write_object::<_, _, 500>(&mut stream, &Ok::<_, String>(())).await?;
                        log::info!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} => start copy_bidirectional");
                        match copy_bidirectional(&mut next_stream, &mut stream).await {
                            Ok(stats) => {
                                log::info!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} done {stats:?}");
                            }
                            Err(err) => {
                                log::error!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} err {err}");
                            }
                        }
                        Ok(())
                    }
                    Err(err) => {
                        log::error!("[PeerConnectionInternal {remote}] stream service {service} source {source} to dest {dest} => open bi error {err}");
                        write_object::<_, _, 500>(&mut stream, &Err::<(), _>(err.to_string())).await?;
                        Err(err)
                    }
                }
            } else {
                log::warn!("[PeerConnectionInternal {remote}] new stream with service {service} source {source} to dest {dest} => but connection for next {next} not found");
                write_object::<_, _, 500>(&mut stream, &Err::<(), _>("route not found".to_string())).await?;
                Err(anyhow!("route not found"))
            }
        }
        None => {
            log::warn!("[PeerConnectionInternal {remote}] new stream with service {service} source {source} to dest {dest} => but route path not found");
            write_object::<_, _, 500>(&mut stream, &Err::<(), _>("route not found".to_string())).await?;
            Err(anyhow!("route not found"))
        }
    }
}
