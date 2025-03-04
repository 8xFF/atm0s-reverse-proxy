use std::{marker::PhantomData, net::SocketAddr, sync::Arc, time::Instant};

use anyhow::anyhow;
use metrics::histogram;
use protocol::{
    key::{ClusterRequest, ClusterValidator},
    proxy::AgentId,
    stream::TunnelStream,
};
use quinn::{Endpoint, Incoming, VarInt};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::de::DeserializeOwned;
use tokio::{
    select,
    sync::mpsc::{channel, Receiver, Sender},
};

use crate::{
    agent::AgentSessionControl,
    quic::{make_server_endpoint, TunnelQuicStream},
    METRICS_AGENT_HISTOGRAM,
};

use super::{AgentListener, AgentListenerEvent, AgentSession, AgentSessionId};

pub struct AgentQuicListener<VALIDATE, HANDSHAKE: ClusterRequest> {
    validate: Arc<VALIDATE>,
    endpoint: Endpoint,
    internal_tx: Sender<AgentListenerEvent<HANDSHAKE::Context, TunnelQuicStream>>,
    internal_rx: Receiver<AgentListenerEvent<HANDSHAKE::Context, TunnelQuicStream>>,
    _tmp: PhantomData<HANDSHAKE>,
}

impl<VALIDATE, HANDSHAKE: ClusterRequest> AgentQuicListener<VALIDATE, HANDSHAKE> {
    pub async fn new(addr: SocketAddr, priv_key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>, validate: VALIDATE) -> anyhow::Result<Self> {
        log::info!("[AgentQuic] starting with addr {addr}");
        let endpoint = make_server_endpoint(addr, priv_key, cert)?;
        let (internal_tx, internal_rx) = channel(10);

        Ok(Self {
            endpoint,
            internal_tx,
            internal_rx,
            validate: validate.into(),
            _tmp: PhantomData,
        })
    }
}

impl<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Send + Sync + 'static + ClusterRequest> AgentListener<REQ::Context, TunnelQuicStream> for AgentQuicListener<VALIDATE, REQ> {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<REQ::Context, TunnelQuicStream>> {
        loop {
            select! {
                incoming = self.endpoint.accept() => {
                    let validate = self.validate.clone();
                    let internal_tx = self.internal_tx.clone();
                    let incoming = incoming.ok_or(anyhow!("quinn crash"))?;
                    let remote = incoming.remote_address();
                    tokio::spawn(async move {
                        if let Err(e) = run_connection(validate, incoming, internal_tx).await {
                            log::error!("[AgentQuic] connection {remote} error {e:?}");
                        }
                    });
                },
                event = self.internal_rx.recv() => break Ok(event.expect("should work")),
            }
        }
    }

    async fn shutdown(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), "Shutdown".as_bytes());
    }
}

async fn run_connection<VALIDATE: ClusterValidator<REQ>, REQ: ClusterRequest>(
    validate: Arc<VALIDATE>,
    incoming: Incoming,
    internal_tx: Sender<AgentListenerEvent<REQ::Context, TunnelQuicStream>>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    log::info!("[AgentQuic] new connection from {}", incoming.remote_address());

    let conn = incoming.await?;
    let (mut send, mut recv) = conn.accept_bi().await?;
    let mut buf = [0u8; 4096];
    let buf_len = recv.read(&mut buf).await?.ok_or(anyhow!("no incoming data"))?;

    log::info!("[AgentQuic] new connection got handhsake data {buf_len} bytes");

    let req = validate.validate_connect_req(&buf[0..buf_len])?;
    let domain = validate.generate_domain(&req)?;
    let agent_id = AgentId::try_from_domain(&domain)?;
    let session_id = AgentSessionId::rand();
    let agent_ctx = req.context();

    log::info!("[AgentQuic] new connection validated with domain {domain} agent_id: {agent_id}, session uuid: {session_id}");

    let res_buf = validate.sign_response_res(&req, None);
    send.write_all(&res_buf).await?;
    let (control_tx, mut control_rx) = channel(10);

    internal_tx
        .send(AgentListenerEvent::Connected(agent_id, AgentSession::new(agent_id, session_id, domain, control_tx)))
        .await
        .expect("should send to main loop");

    log::info!("[AgentQuic] new connection {agent_id} {session_id}  started loop");
    histogram!(METRICS_AGENT_HISTOGRAM).record(started.elapsed().as_millis() as f32 / 1000.0);

    loop {
        select! {
            control = control_rx.recv() => match control {
                Some(control) => match control {
                    AgentSessionControl::CreateStream(tx) => {
                        log::info!("[AgentQuic] agent {agent_id} {session_id} create stream request");
                        let conn = conn.clone();
                        tokio::spawn(async move {
                            match conn.open_bi().await {
                                Ok((send, recv)) => {
                                    log::info!("[AgentQuic] agent {agent_id} {session_id} created stream");
                                    if let Err(_e) = tx.send(Ok(TunnelStream::new(recv, send))) {
                                        log::error!("[AgentQuic] agent {agent_id} {session_id}  send created stream error");
                                    }
                                },
                                Err(err) => {
                                    if let Err(_e) = tx.send(Err(err.into())) {
                                        log::error!("[AgentQuic] agent {agent_id} {session_id}  send create stream's error, may be internal channel failed");
                                    }
                                },
                            }
                        });
                    },
                },
                None => {
                    break;
                }
            },
            accept = conn.accept_bi() => match accept {
                Ok((send, recv)) => {
                    let stream = TunnelStream::new(recv, send);
                    let internal_tx = internal_tx.clone();
                    let agent_ctx = agent_ctx.clone();
                    tokio::spawn(async move {
                        internal_tx.send(AgentListenerEvent::IncomingStream(agent_id, agent_ctx, stream)).await.expect("should send to main loop");
                    });
                },
                Err(err) => {
                    log::error!("[AgentQuic] agent {agent_id} {session_id} quic connection error {err:?}");
                    break;
                },
            }
        }
    }

    log::info!("[AgentQuic] agent {agent_id} {session_id}  stopped loop");

    internal_tx.send(AgentListenerEvent::Disconnected(agent_id, session_id)).await.expect("should send to main loop");

    Ok(())
}
