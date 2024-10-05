use anyhow::anyhow;
use futures::{SinkExt, StreamExt};
use quinn::{Connection, RecvStream, SendStream};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
};
use tokio_util::codec::Framed;

use crate::{
    msg::PeerMessage,
    stream::{BincodeCodec, QuicStream},
    InternalEvent, PeerAddress,
};

use super::PeerConnectionControl;

pub struct PeerConnectionInternal {
    remote: PeerAddress,
    connection: Connection,
    framed: Framed<QuicStream, BincodeCodec<PeerMessage>>,
    internal_tx: Sender<InternalEvent>,
    control_rx: Receiver<PeerConnectionControl>,
}

impl PeerConnectionInternal {
    pub fn new(connection: Connection, main_send: SendStream, main_recv: RecvStream, internal_tx: Sender<InternalEvent>, control_rx: Receiver<PeerConnectionControl>) -> Self {
        let stream = QuicStream::new(main_recv, main_send);
        Self {
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
                let frame = frame.ok_or(anyhow!("peer main stream ended"))??;
                self.internal_tx.send(InternalEvent::PeerData(self.remote, frame)).await.expect("should send to main loop");
                Ok(())
            },
            control = self.control_rx.recv() => {
                let control = control.ok_or(anyhow!("peer control channel ended"))?;
                self.on_control(control).await
            }
        }
    }

    async fn on_accept_bi(&mut self, send: SendStream, recv: RecvStream) -> anyhow::Result<()> {
        todo!()
    }

    async fn on_control(&mut self, control: PeerConnectionControl) -> anyhow::Result<()> {
        match control {
            PeerConnectionControl::Send(item) => Ok(self.framed.send(item).await?),
        }
    }
}
