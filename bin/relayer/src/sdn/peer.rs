use quinn::{Connecting, Incoming};

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct ConnectionId(u64);

pub struct PeerConnection {}

impl PeerConnection {
    pub fn new(incoming: Incoming) -> Self {
        todo!()
    }

    pub fn conn_id(&self) -> ConnectionId {
        todo!()
    }
}
