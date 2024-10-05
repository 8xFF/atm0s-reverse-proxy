use std::{collections::HashMap, net::SocketAddr, time::Duration};

use alias::AliasGuard;
use anyhow::anyhow;
use derive_more::derive::{Deref, Display, From};
use discovery::PeerDiscovery;
use msg::PeerMessage;
use peer::PeerConnection;
use protocol::{stream::TunnelStream, time::now_ms};
use quinn::{Endpoint, Incoming, RecvStream, SendStream, VarInt};
use router::SharedRouterTable;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use tokio::{
    select,
    sync::{
        mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::Interval,
};

use crate::quic::{make_server_endpoint, TunnelQuicStream};

mod alias;
mod discovery;
mod msg;
mod peer;
mod router;

#[derive(Debug, Display, Clone, Copy, From, Deref, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PeerAddress(SocketAddr);

pub struct P2pNetworkRequester {
    control_tx: UnboundedSender<ControlCmd>,
}

enum InternalEvent {
    PeerConnected(PeerAddress, u16),
    PeerConnectError(PeerAddress, anyhow::Error),
    PeerData(PeerAddress, PeerMessage),
    PeerStream(PeerAddress, TunnelQuicStream),
    PeerDisconnected(PeerAddress),
}

enum ControlCmd {
    Connect(PeerAddress, Option<oneshot::Sender<anyhow::Result<()>>>),
    RegisterAlias(u64, oneshot::Sender<AliasGuard>),
    FindAlias(u64, oneshot::Sender<anyhow::Result<Option<PeerAddress>>>),
    CreateStream(PeerAddress, oneshot::Sender<anyhow::Result<TunnelQuicStream>>),
}

#[derive(Default)]
struct NetworkNeighbours {
    peers: HashMap<PeerAddress, PeerConnection>,
}

pub enum P2pNetworkEvent {
    IncomingStream(PeerAddress, TunnelQuicStream),
    Continue,
}

pub struct P2pNetwork {
    endpoint: Endpoint,
    control_tx: UnboundedSender<ControlCmd>,
    control_rx: UnboundedReceiver<ControlCmd>,
    internal_tx: Sender<InternalEvent>,
    internal_rx: Receiver<InternalEvent>,
    neighbours: NetworkNeighbours,
    ticker: Interval,
    router: SharedRouterTable,
    discovery: PeerDiscovery,
}

impl P2pNetwork {
    pub async fn new(addr: SocketAddr, advertise: Option<SocketAddr>, priv_key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>) -> anyhow::Result<Self> {
        log::info!("[P2pNetwork] starting in port {addr}");
        let endpoint = make_server_endpoint(addr, priv_key, cert)?;
        let (internal_tx, internal_rx) = channel(10);
        let (control_tx, control_rx) = unbounded_channel();
        let mut discovery = PeerDiscovery::default();

        if let Some(addr) = advertise {
            discovery.enable_local(addr.into());
        }

        Ok(Self {
            endpoint,
            neighbours: NetworkNeighbours::default(),
            internal_tx,
            internal_rx,
            control_tx,
            control_rx,
            ticker: tokio::time::interval(Duration::from_secs(2)),
            router: SharedRouterTable::default(),
            discovery,
        })
    }

    pub fn requester(&mut self) -> P2pNetworkRequester {
        P2pNetworkRequester { control_tx: self.control_tx.clone() }
    }

    pub async fn recv(&mut self) -> anyhow::Result<P2pNetworkEvent> {
        select! {
            _ = self.ticker.tick() => {
                self.process_tick(now_ms())
            }
            connecting = self.endpoint.accept() => {
                self.process_incoming(connecting.ok_or(anyhow!("quic crash"))?)
            },
            event = self.internal_rx.recv() => {
                self.process_internal(now_ms(), event.ok_or(anyhow!("internal channel crash"))?)
            },
            event = self.control_rx.recv() => {
                self.process_control(event.ok_or(anyhow!("internal channel crash"))?)
            },

        }
    }

    pub fn shutdown(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), "Shutdown".as_bytes());
    }

    fn process_tick(&mut self, now_ms: u64) -> anyhow::Result<P2pNetworkEvent> {
        self.discovery.clear_timeout(now_ms);
        for peer in self.neighbours.connected_peers() {
            let route: router::RouterTableSync = self.router.create_sync(&peer.remote());
            let advertise = self.discovery.create_sync_for(now_ms, &peer.remote());
            if let Err(e) = peer.try_send(PeerMessage::Sync { route, advertise }) {
                log::error!("[P2pNetwork] try send message to peer {} error {e}", peer.remote());
            }
        }
        for addr in self.discovery.remotes() {
            self.control_tx.send(ControlCmd::Connect(*addr, None))?;
        }
        Ok(P2pNetworkEvent::Continue)
    }

    fn process_incoming(&mut self, incoming: Incoming) -> anyhow::Result<P2pNetworkEvent> {
        let remote: PeerAddress = incoming.remote_address().into();
        if self.neighbours.has_peer(&remote) {
            log::warn!("[P2pNetwork] incoming connect from {remote} but already existed => reject");
            incoming.refuse();
            Ok(P2pNetworkEvent::Continue)
        } else {
            log::info!("[P2pNetwork] incoming connect from {remote} => accept");
            self.neighbours.insert(remote, PeerConnection::new_incoming(incoming, self.internal_tx.clone()));
            Ok(P2pNetworkEvent::Continue)
        }
    }

    fn process_internal(&mut self, now_ms: u64, event: InternalEvent) -> anyhow::Result<P2pNetworkEvent> {
        match event {
            InternalEvent::PeerConnected(remote, ttl_ms) => {
                log::info!("[P2pNetwork] connected to {remote}");
                self.router.set_direct(remote, ttl_ms);
                self.neighbours.mark_connected(&remote);
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerData(remote, data) => {
                log::debug!("[P2pNetwork] on data {data:?} from {remote}");
                match data {
                    PeerMessage::Hello {} => {}
                    PeerMessage::Sync { route, advertise } => {
                        self.router.apply_sync(remote, route);
                        self.discovery.apply_sync(now_ms, advertise);
                    }
                }
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerStream(remote, stream) => {
                log::info!("[P2pNetwork] on new stream from {remote}");
                Ok(P2pNetworkEvent::IncomingStream(remote, stream))
            }
            InternalEvent::PeerConnectError(remote, err) => {
                log::error!("[P2pNetwork] connect to {remote} error {err}");
                self.neighbours.remove(&remote);
                Ok(P2pNetworkEvent::Continue)
            }
            InternalEvent::PeerDisconnected(remote) => {
                log::info!("[P2pNetwork] disconnected from {remote}");
                self.router.del_direct(&remote);
                self.neighbours.remove(&remote);
                Ok(P2pNetworkEvent::Continue)
            }
        }
    }

    fn process_control(&mut self, cmd: ControlCmd) -> anyhow::Result<P2pNetworkEvent> {
        match cmd {
            ControlCmd::Connect(addr, tx) => {
                let res = if self.neighbours.has_peer(&addr) {
                    Ok(())
                } else {
                    log::info!("[P2pNetwork] connecting to {addr}");
                    match self.endpoint.connect(*addr, "cluster") {
                        Ok(connecting) => {
                            self.neighbours.insert(addr, PeerConnection::new_connecting(connecting, self.internal_tx.clone()));
                            Ok(())
                        }
                        Err(err) => Err(err.into()),
                    }
                };

                if let Some(tx) = tx {
                    tx.send(res);
                }

                Ok(P2pNetworkEvent::Continue)
            }
            ControlCmd::RegisterAlias(_, sender) => todo!(),
            ControlCmd::FindAlias(_, sender) => todo!(),
            ControlCmd::CreateStream(_, sender) => todo!(),
        }
    }
}

impl P2pNetworkRequester {
    pub async fn connect(&self, addr: PeerAddress) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::Connect(addr, Some(tx))).expect("should send to main loop");
        rx.await?
    }

    pub async fn register_alias(&self, alias: u64) -> AliasGuard {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::RegisterAlias(alias, tx)).expect("should send to main loop");
        rx.await.expect("should get from main loop")
    }

    pub async fn find_alias(&self, alias: u64) -> anyhow::Result<Option<PeerAddress>> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::FindAlias(alias, tx)).expect("should send to main loop");
        rx.await?
    }

    pub async fn create_stream_to(&self, dest: PeerAddress) -> anyhow::Result<TunnelStream<RecvStream, SendStream>> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(ControlCmd::CreateStream(dest, tx)).expect("should send to main loop");
        rx.await?
    }
}

impl NetworkNeighbours {
    fn insert(&mut self, peer: PeerAddress, conn: PeerConnection) {
        self.peers.insert(peer, conn);
    }

    fn has_peer(&self, peer: &PeerAddress) -> bool {
        self.peers.contains_key(peer)
    }

    fn mark_connected(&mut self, peer: &PeerAddress) -> Option<()> {
        self.peers.get_mut(peer)?.set_connected();
        Some(())
    }

    fn remove(&mut self, peer: &PeerAddress) -> Option<()> {
        self.peers.remove(&peer)?;
        Some(())
    }

    fn connected_peers(&self) -> impl Iterator<Item = &PeerConnection> {
        self.peers.values().into_iter().filter(|c| c.is_connected())
    }
}
