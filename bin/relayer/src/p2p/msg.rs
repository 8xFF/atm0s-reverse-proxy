use serde::{Deserialize, Serialize};

use super::router::RouterTableSync;

#[derive(Debug, Serialize, Deserialize)]
pub enum PeerMessage {
    Hello {},
    Sync(RouterTableSync),
}
