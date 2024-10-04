use std::{collections::HashMap, net::SocketAddr};

use alias::AliasGuard;
use anyhow::anyhow;
use derive_more::derive::{Deref, Display, From};
use msg::PeerMessage;
use peer::PeerConnection;
use protocol::stream::TunnelStream;
use quinn::{Endpoint, Incoming, RecvStream, SendStream, VarInt};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::{
    select,
    sync::{
        mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};

use crate::quic::{make_server_endpoint, TunnelQuicStream};

mod alias;
mod msg;
mod peer;

#[derive(Debug, Display, Clone, Copy, From, Deref, PartialEq, Eq, Hash)]
pub struct NodeAddress(SocketAddr);

pub struct P2pNetworkRequester {
    control_tx: UnboundedSender<ControlCmd>,
}

enum InternalEvent {
    PeerConnected(NodeAddress),
    PeerConnectError(NodeAddress, anyhow::Error),
    PeerData(NodeAddress, PeerMessage),
    PeerStream(NodeAddress, TunnelQuicStream),
    PeerDisconnected(NodeAddress),
}

enum ControlCmd {
    Connect(NodeAddress, oneshot::Sender<anyhow::Result<()>>),
    RegisterAlias(u64, oneshot::Sender<AliasGuard>),
    FindAlias(u64, oneshot::Sender<anyhow::Result<Option<NodeAddress>>>),
    CreateStream(NodeAddress, oneshot::Sender<anyhow::Result<TunnelQuicStream>>),
}

pub enum P2pNetworkEvent {
    IncomingStream(NodeAddress, TunnelQuicStream),
    Continue,
}

pub struct P2pNetwork {
    endpoint: Endpoint,
    control_tx: UnboundedSender<ControlCmd>,
    control_rx: UnboundedReceiver<ControlCmd>,
    internal_tx: Sender<InternalEvent>,
    internal_rx: Receiver<InternalEvent>,
    peer_conns: HashMap<NodeAddress, PeerConnection>,
}

impl P2pNetwork {
    pub async fn new(addr: SocketAddr, priv_key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>) -> anyhow::Result<Self> {
        log::info!("[P2pNetwork] starting in port {addr}");
        let endpoint = make_server_endpoint(addr, priv_key, cert)?;
        let (internal_tx, internal_rx) = channel(10);
        let (control_tx, control_rx) = unbounded_channel();

        Ok(Self {
            endpoint,
            peer_conns: HashMap::new(),
            internal_tx,
            internal_rx,
            control_tx,
            control_rx,
        })
    }

    pub fn requester(&mut self) -> P2pNetworkRequester {
        P2pNetworkRequester { control_tx: self.control_tx.clone() }
    }

    pub async fn recv(&mut self) -> anyhow::Result<P2pNetworkEvent> {
        select! {
            connecting = self.endpoint.accept() => {
                self.process_incoming(connecting.ok_or(anyhow!("quic crash"))?)
            },
            event = self.internal_rx.recv() => {
                self.process_internal(event.ok_or(anyhow!("internal channel crash"))?)
            },
            event = self.control_rx.recv() => {
                self.process_control(event.ok_or(anyhow!("internal channel crash"))?)
            },

        }
    }

    pub fn shutdown(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), "Shutdown".as_bytes());
    }

    fn process_incoming(&mut self, incoming: Incoming) -> anyhow::Result<P2pNetworkEvent> {
        let remote: NodeAddress = incoming.remote_address().into();
        if self.peer_conns.contains_key(&remote) {
            log::warn!("[P2pNetwork] incoming connect from {remote} but already existed => reject");
            incoming.refuse();
            Ok(P2pNetworkEvent::Continue)
        } else {
            log::info!("[P2pNetwork] incoming connect from {remote} => accept");
            self.peer_conns.insert(remote, PeerConnection::new_incoming(incoming, self.internal_tx.clone()));
            Ok(P2pNetworkEvent::Continue)
        }
    }

    fn process_internal(&mut self, event: InternalEvent) -> anyhow::Result<P2pNetworkEvent> {
        match event {
            InternalEvent::PeerConnected(remote) => {
                log::info!("[P2pNetwork] connected to {remote}");
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerData(remote, data) => {
                log::info!("[P2pNetwork] on data {data:?} from {remote}");
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerConnectError(remote, err) => {
                log::error!("[P2pNetwork] connect to {remote} error {err:?}");
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerDisconnected(remote) => {
                log::info!("[P2pNetwork] disconnected from {remote}");
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerStream(remote, stream) => {
                log::info!("[P2pNetwork] on new stream from {remote}");
                Ok(P2pNetworkEvent::IncomingStream(remote, stream))
            }
        }
    }

    fn process_control(&mut self, cmd: ControlCmd) -> anyhow::Result<P2pNetworkEvent> {
        match cmd {
            ControlCmd::Connect(addr, tx) => {
                if self.peer_conns.contains_key(&addr) {
                    tx.send(Ok(()));
                } else {
                    log::info!("[P2pNetwork] connecting to {addr}");
                    match self.endpoint.connect(*addr, "cluster") {
                        Ok(connecting) => {
                            self.peer_conns.insert(addr, PeerConnection::new_connecting(connecting, self.internal_tx.clone()));
                            tx.send(Ok(()));
                        }
                        Err(err) => {
                            tx.send(Err(err.into()));
                        }
                    }
                }

                Ok(P2pNetworkEvent::Continue)
            }
            ControlCmd::RegisterAlias(_, sender) => todo!(),
            ControlCmd::FindAlias(_, sender) => todo!(),
            ControlCmd::CreateStream(node_id, sender) => todo!(),
        }
    }
}

impl P2pNetworkRequester {
    pub async fn connect(&self, addr: NodeAddress) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::Connect(addr, tx)).expect("should send to main loop");
        rx.await?
    }

    pub async fn register_alias(&self, alias: u64) -> AliasGuard {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::RegisterAlias(alias, tx)).expect("should send to main loop");
        rx.await.expect("should get from main loop")
    }

    pub async fn find_alias(&self, alias: u64) -> anyhow::Result<Option<NodeAddress>> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::FindAlias(alias, tx)).expect("should send to main loop");
        rx.await?
    }

    pub async fn create_stream_to(&self, dest: NodeAddress) -> anyhow::Result<TunnelStream<RecvStream, SendStream>> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::CreateStream(dest, tx)).expect("should send to main loop");
        rx.await?
    }
}
