use serde::{Deserialize, Serialize};

use super::{discovery::PeerDiscoverySync, router::RouterTableSync};

#[derive(Debug, Serialize, Deserialize)]
pub enum PeerMessage {
    Hello {},
    Sync { route: RouterTableSync, advertise: PeerDiscoverySync },
}
