use tokio::sync::{mpsc::UnboundedSender, oneshot};

use crate::{ControlCmd, PeerAddress};

pub struct P2pNetworkRequester {
    pub(crate) control_tx: UnboundedSender<ControlCmd>,
}

impl P2pNetworkRequester {
    pub async fn connect(&self, addr: PeerAddress) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::Connect(addr, Some(tx))).expect("should send to main loop");
        rx.await?
    }
}
