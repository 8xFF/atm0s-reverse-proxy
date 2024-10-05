use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::PeerAddress;

const TIMEOUT_AFTER: u64 = 30_000;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerDiscoverySync(Vec<(PeerAddress, u64)>);

#[derive(Debug, Default)]
pub struct PeerDiscovery {
    local: Option<PeerAddress>,
    remotes: BTreeMap<PeerAddress, u64>,
}

impl PeerDiscovery {
    pub fn enable_local(&mut self, local: PeerAddress) {
        log::info!("[PeerDiscovery] enable local as {local}");
        self.local = Some(local);
    }

    pub fn disable_local(&mut self) {
        log::info!("[PeerDiscovery] disable local");
        self.local = None;
    }

    pub fn clear_timeout(&mut self, now_ms: u64) {
        self.remotes.retain(|peer, last_updated| {
            if *last_updated + TIMEOUT_AFTER > now_ms {
                true
            } else {
                log::info!("[PeerDiscovery] remove timeout {peer}");
                false
            }
        });
    }

    pub fn create_sync_for(&self, now_ms: u64, dest: &PeerAddress) -> PeerDiscoverySync {
        let iter = self.local.iter().map(|p| (*p, now_ms));
        PeerDiscoverySync(self.remotes.iter().filter(|(k, _)| !dest.eq(k)).map(|(k, v)| (*k, *v)).chain(iter).collect::<Vec<_>>())
    }

    pub fn apply_sync(&mut self, now_ms: u64, sync: PeerDiscoverySync) {
        for (peer, last_updated) in sync.0.into_iter() {
            if last_updated + TIMEOUT_AFTER > now_ms {
                if self.remotes.insert(peer, last_updated).is_none() {
                    log::info!("[PeerDiscovery] added new peer {peer}");
                }
            }
        }
    }

    pub fn remotes(&self) -> impl Iterator<Item = &PeerAddress> {
        self.remotes.keys().into_iter()
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use crate::{discovery::PeerDiscoverySync, PeerAddress};

    use super::{PeerDiscovery, TIMEOUT_AFTER};

    fn create_peer(i: u16) -> PeerAddress {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), i).into()
    }

    #[test_log::test]
    fn create_local_sync() {
        let mut discovery = PeerDiscovery::default();

        let peer1 = create_peer(1);
        let peer2 = create_peer(1);

        assert_eq!(discovery.create_sync_for(0, &peer2), PeerDiscoverySync(vec![]));

        discovery.enable_local(peer1);

        assert_eq!(discovery.create_sync_for(100, &peer2), PeerDiscoverySync(vec![(peer1, 100)]));
        assert_eq!(discovery.remotes().next(), None);
    }

    #[test_log::test]
    fn apply_sync() {
        let mut discovery = PeerDiscovery::default();

        let peer1 = create_peer(1);
        let peer2 = create_peer(2);

        discovery.apply_sync(100, PeerDiscoverySync(vec![(peer1, 90)]));

        assert_eq!(discovery.create_sync_for(100, &peer2), PeerDiscoverySync(vec![(peer1, 90)]));
        assert_eq!(discovery.create_sync_for(100, &peer1), PeerDiscoverySync(vec![]));
        assert_eq!(discovery.remotes().collect::<Vec<_>>(), vec![&peer1]);
    }

    #[test_log::test]
    fn apply_sync_timeout() {
        let mut discovery = PeerDiscovery::default();

        let peer1 = create_peer(1);
        let peer2 = create_peer(2);

        discovery.apply_sync(TIMEOUT_AFTER + 100, PeerDiscoverySync(vec![(peer1, 100)]));

        assert_eq!(discovery.create_sync_for(100, &peer2), PeerDiscoverySync(vec![]));
        assert_eq!(discovery.create_sync_for(100, &peer1), PeerDiscoverySync(vec![]));
        assert_eq!(discovery.remotes().next(), None);
    }

    #[test_log::test]
    fn clear_timeout() {
        let mut discovery = PeerDiscovery::default();

        let peer1 = create_peer(1);

        discovery.apply_sync(100, PeerDiscoverySync(vec![(peer1, 90)]));

        assert_eq!(discovery.remotes().collect::<Vec<_>>(), vec![&peer1]);

        discovery.clear_timeout(TIMEOUT_AFTER + 90);

        assert_eq!(discovery.remotes().next(), None);
    }
}
