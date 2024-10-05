//! Simple p2p router table
//! The idea behind it is a spread routing, we allow some route loop then it is resolve by 2 method:
//!
//! - Direct rtt always has lower rtt
//! - MAX_HOPS will reject some loop after direct connection disconnected

use std::{collections::BTreeMap, ops::AddAssign, sync::Arc};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::PeerAddress;

const MAX_HOPS: u8 = 6;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
pub struct PathMetric {
    relay_hops: u8,
    rtt_ms: u16,
}

impl From<(u8, u16)> for PathMetric {
    fn from(value: (u8, u16)) -> Self {
        Self { relay_hops: value.0, rtt_ms: value.1 }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouterTableSync(Vec<(PeerAddress, PathMetric)>);

#[derive(Default)]
struct PeerMemory {
    best: Option<PeerAddress>,
    paths: BTreeMap<PeerAddress, PathMetric>,
}

#[derive(Default)]
pub struct RouterTable {
    peers: BTreeMap<PeerAddress, PeerMemory>,
    directs: BTreeMap<PeerAddress, PathMetric>,
}

impl RouterTable {
    pub fn create_sync(&self, dest: &PeerAddress) -> RouterTableSync {
        RouterTableSync(
            self.peers
                .iter()
                .map(|(addr, history)| (*addr, history.best_metric().expect("should have best")))
                .filter(|(addr, metric)| !dest.eq(addr) && metric.relay_hops <= MAX_HOPS)
                .collect::<Vec<_>>(),
        )
    }

    pub fn apply_sync(&mut self, from: PeerAddress, sync: RouterTableSync) {
        let direct_metric = self.directs.get(&from).expect("should have direct metric with apply_sync");
        // ensure we have memory for each sync paths
        for (peer, _) in sync.0.iter() {
            self.peers.entry(*peer).or_default();
        }

        let mut new_paths = BTreeMap::<PeerAddress, PathMetric>::from_iter(sync.0.into_iter());
        // only loop over peer which don't equal source, because it is direct connection
        for (peer, memory) in self.peers.iter_mut().filter(|(p, _)| !from.eq(p)) {
            let previous = memory.paths.contains_key(&from);
            let current = new_paths.remove(peer);
            match (previous, current) {
                (true, Some(mut new_metric)) => {
                    // has update
                    new_metric += *direct_metric;
                    memory.paths.insert(from, new_metric);
                    Self::select_best_for(peer, memory);
                }
                (true, None) => {
                    // delete
                    log::info!("[RouterTable] remove path for {peer}");
                    memory.paths.remove(&from);
                    Self::select_best_for(peer, memory);
                }
                (false, Some(mut new_metric)) => {
                    // new create
                    log::info!("[RouterTable] create path for {peer}");
                    new_metric += *direct_metric;
                    memory.paths.insert(from, new_metric);
                    Self::select_best_for(peer, memory);
                }
                _ => { //dont changed
                }
            }
        }
        self.peers.retain(|_k, v| v.best().is_some());
    }

    pub fn set_direct(&mut self, from: PeerAddress, ttl_ms: u16) {
        self.directs.insert(from, (1, ttl_ms).into());
        let memory = self.peers.entry(from).or_default();
        memory.paths.insert(from, PathMetric { relay_hops: 0, rtt_ms: ttl_ms });
        Self::select_best_for(&from, memory);
    }

    pub fn del_direct(&mut self, from: &PeerAddress) {
        self.directs.remove(&from);
        if let Some(memory) = self.peers.get_mut(from) {
            memory.paths.remove(&from);
            Self::select_best_for(&from, memory);
            if memory.best().is_none() {
                self.peers.remove(&from);
            }
        }
    }

    pub fn next(&self, next: PeerAddress) -> Option<PeerAddress> {
        self.peers.get(&next)?.best()
    }

    pub fn next_with_details(&self, next: PeerAddress) -> Option<(PeerAddress, PathMetric)> {
        let memory = self.peers.get(&next)?;
        let best = memory.best()?;
        let metric = memory.best_metric().expect("should have metric");
        Some((best, metric))
    }

    fn select_best_for(dest: &PeerAddress, memory: &mut PeerMemory) {
        if let Some((new_best, metric)) = memory.select_best() {
            log::info!(
                "[RouterTable] to {dest} select new path over {new_best} with rtt {} ms over {} hop(s)",
                metric.rtt_ms,
                metric.relay_hops
            );
        }
    }
}

impl PathMetric {
    fn score(&self) -> u16 {
        self.rtt_ms + self.relay_hops as u16 * 10
    }
}

impl AddAssign for PathMetric {
    fn add_assign(&mut self, rhs: Self) {
        self.relay_hops += rhs.relay_hops;
        self.rtt_ms += rhs.rtt_ms;
    }
}

impl PeerMemory {
    fn best(&self) -> Option<PeerAddress> {
        self.best
    }

    fn best_metric(&self) -> Option<PathMetric> {
        self.best.map(|p| *self.paths.get(&p).expect("should have metric with best path"))
    }

    fn select_best(&mut self) -> Option<(PeerAddress, PathMetric)> {
        let previous = self.best;
        self.best = None;
        let mut iter = self.paths.iter();
        let (peer, metric) = iter.next()?;
        let mut best_peer = peer;
        let mut best_score = metric.score();

        for (peer, metric) in iter {
            if best_score > metric.score() {
                best_peer = peer;
                best_score = metric.score();
            }
        }

        self.best = Some(*best_peer);
        if self.best != previous {
            let metric = self.best_metric().expect("should have best metric after select success");
            Some((*best_peer, metric))
        } else {
            None
        }
    }
}

#[derive(Default, Clone)]
pub struct SharedRouterTable {
    table: Arc<RwLock<RouterTable>>,
}

impl SharedRouterTable {
    pub fn create_sync(&self, dest: &PeerAddress) -> RouterTableSync {
        self.table.read().create_sync(&dest)
    }

    pub fn apply_sync(&self, from: PeerAddress, sync: RouterTableSync) {
        self.table.write().apply_sync(from, sync);
    }

    pub fn set_direct(&self, from: PeerAddress, ttl_ms: u16) {
        self.table.write().set_direct(from, ttl_ms);
    }

    pub fn del_direct(&self, from: &PeerAddress) {
        self.table.write().del_direct(from);
    }

    pub fn next(&self, next: PeerAddress) -> Option<PeerAddress> {
        self.table.read().next(next)
    }

    pub fn next_with_details(&self, next: PeerAddress) -> Option<(PeerAddress, PathMetric)> {
        self.table.read().next_with_details(next)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use crate::{p2p::router::RouterTableSync, PeerAddress};

    use super::{RouterTable, MAX_HOPS};

    fn create_peer(i: u16) -> PeerAddress {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), i).into()
    }

    #[test_log::test]
    fn create_correct_direct_sync() {
        let mut table = RouterTable::default();

        let peer1 = create_peer(1);
        let peer2 = create_peer(2);
        let peer3 = create_peer(3);

        table.set_direct(peer1, 100);
        table.set_direct(peer2, 200);

        assert_eq!(table.next_with_details(peer1), Some((peer1, (0, 100).into())));
        assert_eq!(table.next_with_details(peer2), Some((peer2, (0, 200).into())));
        assert_eq!(table.next_with_details(peer3), None);

        assert_eq!(table.create_sync(&peer1), RouterTableSync(vec![(peer2, (0, 200).into())]));
        assert_eq!(table.create_sync(&peer2), RouterTableSync(vec![(peer1, (0, 100).into())]));
    }

    #[test_log::test]
    fn apply_correct_direct_sync() {
        let mut table = RouterTable::default();

        let peer1 = create_peer(1);
        let peer2 = create_peer(2);
        let peer3 = create_peer(3);
        let peer4 = create_peer(4);

        table.set_direct(peer1, 100);
        table.set_direct(peer4, 400);

        table.apply_sync(peer1, RouterTableSync(vec![(peer2, (0, 200).into())]));

        // now we have NODO => peer1 => peer2
        assert_eq!(table.next_with_details(peer1), Some((peer1, (0, 100).into())));
        assert_eq!(table.next_with_details(peer2), Some((peer1, (1, 300).into())));
        assert_eq!(table.next(peer3), None);

        // we seems to have loop with peer2 here but it will not effect to routing because we have direct connection, it will always has lower rtt
        assert_eq!(table.create_sync(&peer1), RouterTableSync(vec![(peer2, (1, 300).into()), (peer4, (0, 400).into())]));
        assert_eq!(table.create_sync(&peer4), RouterTableSync(vec![(peer1, (0, 100).into()), (peer2, (1, 300).into())]));
    }

    #[test_log::test]
    fn dont_create_sync_over_max_hops() {
        let mut table = RouterTable::default();

        let peer1 = create_peer(1);
        let peer2 = create_peer(2);
        let peer3 = create_peer(3);

        table.set_direct(peer1, 100);
        table.set_direct(peer3, 300);

        table.apply_sync(peer1, RouterTableSync(vec![(peer2, (MAX_HOPS, 200).into())]));
        assert_eq!(table.next_with_details(peer2), Some((peer1, (MAX_HOPS + 1, 300).into())));

        // we shouldn't create sync with peer2 because it over MAX_HOPS
        assert_eq!(table.create_sync(&peer3), RouterTableSync(vec![(peer1, (0, 100).into())]));
    }
}
