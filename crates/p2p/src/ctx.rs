use std::{collections::HashMap, sync::Arc};

use lru::LruCache;
use parking_lot::RwLock;
use tokio::sync::mpsc::Sender;

use crate::{
    msg::{BroadcastMsgId, ServiceId},
    peer::PeerAlias,
    router::SharedRouterTable,
    service::P2pServiceEvent,
    PeerAddress,
};

#[derive(Debug)]
struct Ctx {
    peers: HashMap<PeerAddress, PeerAlias>,
    received_broadcast_msg: LruCache<BroadcastMsgId, ()>,
}

impl Ctx {
    pub fn register_peer(&mut self, peer: PeerAddress, alias: PeerAlias) {
        self.peers.insert(peer, alias);
    }

    pub fn unregister_peer(&mut self, peer: &PeerAddress) {
        self.peers.remove(peer);
    }

    pub fn peer(&self, peer: &PeerAddress) -> Option<PeerAlias> {
        self.peers.get(peer).cloned()
    }

    pub fn peers(&self) -> Vec<PeerAlias> {
        self.peers.values().cloned().collect::<Vec<_>>()
    }

    /// check if we already got the message
    /// if is not, it return true and save to cache list
    /// if already it return false and do nothing
    pub fn check_broadcast_msg(&mut self, id: BroadcastMsgId) -> bool {
        if self.received_broadcast_msg.contains(&id) {
            self.received_broadcast_msg.get_or_insert(id, || ());
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedCtx {
    ctx: Arc<RwLock<Ctx>>,
    router: SharedRouterTable,
    services: [Option<Sender<P2pServiceEvent>>; 256],
}

impl SharedCtx {
    pub fn new(router: SharedRouterTable, services: [Option<Sender<P2pServiceEvent>>; 256]) -> Self {
        Self {
            ctx: Arc::new(RwLock::new(Ctx {
                peers: Default::default(),
                received_broadcast_msg: LruCache::new(8192.try_into().expect("should ok")),
            })),
            router,
            services,
        }
    }

    pub fn register_peer(&self, peer: PeerAddress, alias: PeerAlias) {
        self.ctx.write().register_peer(peer, alias);
    }

    pub fn unregister_peer(&self, peer: &PeerAddress) {
        self.ctx.write().unregister_peer(peer);
    }

    pub fn peer(&self, peer: &PeerAddress) -> Option<PeerAlias> {
        self.ctx.read().peer(peer)
    }

    pub fn peers(&self) -> Vec<PeerAlias> {
        self.ctx.read().peers()
    }

    pub fn router(&self) -> &SharedRouterTable {
        &self.router
    }

    pub fn service(&self, service_id: &ServiceId) -> Option<&Sender<P2pServiceEvent>> {
        self.services[**service_id as usize].as_ref()
    }

    /// check if we already got the message
    /// if is not, it return true and save to cache list
    /// if already it return false and do nothing
    pub fn check_broadcast_msg(&mut self, id: BroadcastMsgId) -> bool {
        self.ctx.write().check_broadcast_msg(id)
    }
}
