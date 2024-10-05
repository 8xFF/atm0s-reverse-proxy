//! PeerAlias allow control a peer-connection from othert task
//! This is done by using control_tx to send control to running task over chanel

use tokio::sync::{mpsc::Sender, oneshot};

use crate::{
    msg::{PeerMessage, ServiceId},
    stream::P2pQuicStream,
    PeerAddress,
};

use super::PeerConnectionControl;

#[derive(Clone, Debug)]
pub struct PeerAlias {
    peer: PeerAddress,
    control_tx: Sender<PeerConnectionControl>,
}

impl PeerAlias {
    pub(super) fn new(peer: PeerAddress, control_tx: Sender<PeerConnectionControl>) -> Self {
        Self { peer, control_tx }
    }

    pub(super) async fn send(&self, control: PeerConnectionControl) -> anyhow::Result<()> {
        self.control_tx.send(control).await?;
        Ok(())
    }

    pub(super) fn address(&self) -> PeerAddress {
        self.peer
    }

    pub(crate) fn try_send(&self, msg: PeerMessage) -> anyhow::Result<()> {
        Ok(self.control_tx.try_send(PeerConnectionControl::Send(msg))?)
    }

    pub(super) async fn open_stream(&self, service: ServiceId, source: PeerAddress, dest: PeerAddress, meta: Vec<u8>) -> anyhow::Result<P2pQuicStream> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(PeerConnectionControl::OpenStream(service, source, dest, meta, tx)).await?;
        Ok(rx.await??)
    }
}
