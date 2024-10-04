use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum PeerMessage {
    Hello {},
    Sync {},
}
