use std::{net::SocketAddr, time::Duration};

use anyhow::anyhow;
use ctx::SharedCtx;
use derive_more::derive::{Deref, Display, From};
use discovery::{PeerDiscovery, PeerDiscoverySync};
use msg::PeerMessage;
use neighbours::NetworkNeighbours;
use peer::PeerConnection;
use quinn::{Endpoint, Incoming, VarInt};
use router::{RouterTableSync, SharedRouterTable};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use stream::P2pQuicStream;
use tokio::{
    select,
    sync::{
        mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::Interval,
};
use utils::{now_ms, ErrorExt2};

use crate::quic::make_server_endpoint;

mod alias;
mod ctx;
mod discovery;
mod msg;
mod neighbours;
mod peer;
mod quic;
mod requester;
mod router;
mod service;
mod stream;
mod utils;

pub use alias::AliasGuard;
pub use requester::P2pNetworkRequester;
pub use service::{P2pService, P2pServiceBuilder, P2pServiceEvent, P2pServiceRequester};

#[derive(Debug, Display, Clone, Copy, From, Deref, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PeerAddress(SocketAddr);

#[derive(Debug)]
enum PeerMainData {
    Sync { route: RouterTableSync, advertise: PeerDiscoverySync },
}

enum InternalEvent {
    PeerConnected(PeerAddress, u16),
    PeerConnectError(PeerAddress, anyhow::Error),
    PeerData(PeerAddress, PeerMainData),
    PeerDisconnected(PeerAddress),
}

enum ControlCmd {
    Connect(PeerAddress, Option<oneshot::Sender<anyhow::Result<()>>>),
    RegisterAlias(u64, oneshot::Sender<AliasGuard>),
    FindAlias(u64, oneshot::Sender<anyhow::Result<Option<PeerAddress>>>),
    CreateStream(PeerAddress, oneshot::Sender<anyhow::Result<P2pQuicStream>>),
}

pub struct P2pNetworkConfig {
    pub addr: SocketAddr,
    pub advertise: Option<SocketAddr>,
    pub priv_key: PrivatePkcs8KeyDer<'static>,
    pub cert: CertificateDer<'static>,
    pub services: P2pServiceBuilder,
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
    ctx: SharedCtx,
}

impl P2pNetwork {
    pub async fn new(cfg: P2pNetworkConfig) -> anyhow::Result<Self> {
        log::info!("[P2pNetwork] starting in port {}", cfg.addr);
        let endpoint = make_server_endpoint(cfg.addr, cfg.priv_key, cfg.cert)?;
        let (internal_tx, internal_rx) = channel(10);
        let (control_tx, control_rx) = unbounded_channel();
        let mut discovery = PeerDiscovery::default();
        let router = SharedRouterTable::new(cfg.addr.into());

        if let Some(addr) = cfg.advertise {
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
            ctx: SharedCtx::new(router.clone(), cfg.services.build()),
            router,
            discovery,
        })
    }

    pub fn requester(&mut self) -> P2pNetworkRequester {
        P2pNetworkRequester { control_tx: self.control_tx.clone() }
    }

    pub async fn recv(&mut self) -> anyhow::Result<()> {
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

    fn process_tick(&mut self, now_ms: u64) -> anyhow::Result<()> {
        self.discovery.clear_timeout(now_ms);
        for peer in self.neighbours.connected_peers() {
            let route: router::RouterTableSync = self.router.create_sync(&peer.remote());
            let advertise = self.discovery.create_sync_for(now_ms, &peer.remote());
            if let Some(alias) = self.ctx.peer(&peer.remote()) {
                if let Err(e) = alias.try_send(PeerMessage::Sync { route, advertise }) {
                    log::error!("[P2pNetwork] try send message to peer {} error {e}", peer.remote());
                }
            }
        }
        for addr in self.discovery.remotes() {
            self.control_tx.send(ControlCmd::Connect(*addr, None))?;
        }
        Ok(())
    }

    fn process_incoming(&mut self, incoming: Incoming) -> anyhow::Result<()> {
        let remote: PeerAddress = incoming.remote_address().into();
        if self.neighbours.has_peer(&remote) {
            log::warn!("[P2pNetwork] incoming connect from {remote} but already existed => reject");
            incoming.refuse();
            Ok(())
        } else {
            log::info!("[P2pNetwork] incoming connect from {remote} => accept");
            self.neighbours.insert(remote, PeerConnection::new_incoming(incoming, self.internal_tx.clone(), self.ctx.clone()));
            Ok(())
        }
    }

    fn process_internal(&mut self, now_ms: u64, event: InternalEvent) -> anyhow::Result<()> {
        match event {
            InternalEvent::PeerConnected(remote, ttl_ms) => {
                log::info!("[P2pNetwork] connected to {remote}");
                self.router.set_direct(remote, ttl_ms);
                self.neighbours.mark_connected(&remote);
                Ok(())
            }
            InternalEvent::PeerData(remote, data) => {
                log::debug!("[P2pNetwork] on data {data:?} from {remote}");
                match data {
                    PeerMainData::Sync { route, advertise } => {
                        self.router.apply_sync(remote, route);
                        self.discovery.apply_sync(now_ms, advertise);
                    }
                }
                Ok(())
            }
            InternalEvent::PeerConnectError(remote, err) => {
                log::error!("[P2pNetwork] connect to {remote} error {err}");
                self.neighbours.remove(&remote);
                Ok(())
            }
            InternalEvent::PeerDisconnected(remote) => {
                log::info!("[P2pNetwork] disconnected from {remote}");
                self.router.del_direct(&remote);
                self.neighbours.remove(&remote);
                Ok(())
            }
        }
    }

    fn process_control(&mut self, cmd: ControlCmd) -> anyhow::Result<()> {
        match cmd {
            ControlCmd::Connect(addr, tx) => {
                let res = if self.neighbours.has_peer(&addr) {
                    Ok(())
                } else {
                    log::info!("[P2pNetwork] connecting to {addr}");
                    match self.endpoint.connect(*addr, "cluster") {
                        Ok(connecting) => {
                            self.neighbours.insert(addr, PeerConnection::new_connecting(connecting, self.internal_tx.clone(), self.ctx.clone()));
                            Ok(())
                        }
                        Err(err) => Err(err.into()),
                    }
                };

                if let Some(tx) = tx {
                    tx.send(res).print_on_err2("[P2pNetwork] send connect answer");
                }

                Ok(())
            }
            ControlCmd::RegisterAlias(_, sender) => todo!(),
            ControlCmd::FindAlias(_, sender) => todo!(),
            ControlCmd::CreateStream(_, sender) => todo!(),
        }
    }
}
