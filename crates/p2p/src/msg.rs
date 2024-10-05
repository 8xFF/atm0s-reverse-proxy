use serde::{Deserialize, Serialize};

use super::{discovery::PeerDiscoverySync, router::RouterTableSync, PeerAddress};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BroadcastMsgId(u64);

impl BroadcastMsgId {
    pub fn rand() -> Self {
        Self(rand::random())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PeerMessage {
    Hello {},
    Sync { route: RouterTableSync, advertise: PeerDiscoverySync },
    // Broadcast(BroadcastMsgId, Vec<u8>),
    // Unicast(PeerAddress, Vec<u8>),
}

pub enum StreamHandshake {
    ConnectReq(PeerAddress),
    ConnectRes(),
}
