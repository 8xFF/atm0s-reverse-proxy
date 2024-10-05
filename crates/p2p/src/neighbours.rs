use std::collections::HashMap;

use crate::{peer::PeerConnection, PeerAddress};

#[derive(Default)]
pub struct NetworkNeighbours {
    peers: HashMap<PeerAddress, PeerConnection>,
}

impl NetworkNeighbours {
    pub fn insert(&mut self, peer: PeerAddress, conn: PeerConnection) {
        self.peers.insert(peer, conn);
    }

    pub fn has_peer(&self, peer: &PeerAddress) -> bool {
        self.peers.contains_key(peer)
    }

    pub fn mark_connected(&mut self, peer: &PeerAddress) -> Option<()> {
        self.peers.get_mut(peer)?.set_connected();
        Some(())
    }

    pub fn remove(&mut self, peer: &PeerAddress) -> Option<()> {
        self.peers.remove(&peer)?;
        Some(())
    }

    pub fn connected_peers(&self) -> impl Iterator<Item = &PeerConnection> {
        self.peers.values().into_iter().filter(|c| c.is_connected())
    }
}
