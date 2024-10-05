use peer_internal::PeerConnectionInternal;
use quinn::{Connecting, Connection, Incoming, RecvStream, SendStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use super::{msg::PeerMessage, InternalEvent, PeerAddress};

mod peer_internal;

enum PeerConnectionControl {
    Send(PeerMessage),
}

pub struct PeerConnection {
    remote: PeerAddress,
    connected: bool,
    control_tx: Sender<PeerConnectionControl>,
}

impl PeerConnection {
    pub fn new_incoming(incoming: Incoming, internal_tx: Sender<InternalEvent>) -> Self {
        let (control_tx, control_rx) = channel(10);
        let remote: PeerAddress = incoming.remote_address().into();
        tokio::spawn(async move {
            log::info!("[PeerConnection] wait incoming from {remote}");
            match incoming.await {
                Ok(connection) => {
                    log::info!("[PeerConnection] got connection from {remote}");
                    match connection.accept_bi().await {
                        Ok((send, recv)) => run_connection(remote, connection, send, recv, internal_tx, control_rx).await,
                        Err(err) => internal_tx.send(InternalEvent::PeerConnectError(remote, err.into())).await.expect("should send to main"),
                    }
                }
                Err(err) => internal_tx.send(InternalEvent::PeerConnectError(remote, err.into())).await.expect("should send to main"),
            }
        });
        Self { remote, connected: false, control_tx }
    }

    pub fn new_connecting(connecting: Connecting, internal_tx: Sender<InternalEvent>) -> Self {
        let (control_tx, control_rx) = channel(10);
        let remote: PeerAddress = connecting.remote_address().into();
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    log::info!("[PeerConnection] connected to {remote}");
                    match connection.open_bi().await {
                        Ok((send, recv)) => run_connection(remote, connection, send, recv, internal_tx, control_rx).await,
                        Err(err) => internal_tx.send(InternalEvent::PeerConnectError(remote, err.into())).await.expect("should send to main"),
                    }
                }
                Err(err) => internal_tx.send(InternalEvent::PeerConnectError(remote, err.into())).await.expect("should send to main"),
            }
        });
        Self { remote, connected: false, control_tx }
    }

    pub fn remote(&self) -> PeerAddress {
        self.remote
    }

    pub fn set_connected(&mut self) {
        self.connected = true;
    }

    pub async fn send(&self, msg: PeerMessage) -> anyhow::Result<()> {
        Ok(self.control_tx.send(PeerConnectionControl::Send(msg)).await?)
    }

    pub fn try_send(&self, msg: PeerMessage) -> anyhow::Result<()> {
        Ok(self.control_tx.try_send(PeerConnectionControl::Send(msg))?)
    }
}

async fn run_connection(remote: PeerAddress, connection: Connection, send: SendStream, recv: RecvStream, internal_tx: Sender<InternalEvent>, control_rx: Receiver<PeerConnectionControl>) {
    let rtt_ms = connection.rtt().as_millis().min(u16::MAX as u128) as u16;
    let mut internal = PeerConnectionInternal::new(connection, send, recv, internal_tx.clone(), control_rx);
    if let Err(e) = internal.start().await {
        log::error!("[PeerConnection] start {remote} response error {e}");
        return;
    }
    log::info!("[PeerConnection] started {remote}, rtt: {rtt_ms}");
    internal_tx.send(InternalEvent::PeerConnected(remote, rtt_ms)).await.expect("should send to main");
    log::info!("[PeerConnection] run loop for {remote}");
    loop {
        if let Err(e) = internal.recv_complex().await {
            log::error!("[PeerConnection] {remote} error {e}");
            break;
        }
    }
    internal_tx.send(InternalEvent::PeerDisconnected(remote)).await.expect("should send to main");
    log::info!("[PeerConnection] end loop for {remote}");
}
