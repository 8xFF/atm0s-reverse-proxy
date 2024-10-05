use derive_more::derive::{Deref, Display, From};
use serde::{Deserialize, Serialize};

use super::{discovery::PeerDiscoverySync, router::RouterTableSync, PeerAddress};

#[derive(Debug, Display, PartialEq, Eq, Hash, Serialize, Deserialize, Clone, Copy)]
pub struct BroadcastMsgId(u64);

#[derive(Debug, Display, PartialEq, Deref, Eq, Hash, Serialize, Deserialize, From, Clone, Copy)]
pub struct ServiceId(u16);

impl BroadcastMsgId {
    pub fn rand() -> Self {
        Self(rand::random())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PeerMessage {
    Hello {},
    Sync { route: RouterTableSync, advertise: PeerDiscoverySync },
    Broadcast(PeerAddress, ServiceId, BroadcastMsgId, Vec<u8>),
    Unicast(PeerAddress, PeerAddress, ServiceId, Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamConnectReq {
    pub source: PeerAddress,
    pub dest: PeerAddress,
    pub service: ServiceId,
    pub meta: Vec<u8>,
}

pub type StreamConnectRes = Result<(), String>;
