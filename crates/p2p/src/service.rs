use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::{alias::AliasGuard, stream::P2pQuicStream, PeerAddress};

const SERVICE_CHANNEL_SIZE: usize = 10;

pub enum P2pServiceEvent {
    Unicast(PeerAddress, Vec<u8>),
    Broadcast(PeerAddress, Vec<u8>),
    Stream(PeerAddress, Vec<u8>, P2pQuicStream),
}

pub struct P2pServiceRequester {}

pub struct P2pService {
    rx: Receiver<P2pServiceEvent>,
}

#[derive(Debug)]
pub struct P2pServiceBuilder {
    services: [Option<Sender<P2pServiceEvent>>; 256],
}

impl Default for P2pServiceBuilder {
    fn default() -> Self {
        Self {
            services: std::array::from_fn(|_| None),
        }
    }
}

impl P2pService {
    pub fn requester(&self) -> P2pServiceRequester {
        todo!()
    }

    pub fn register_alias(&self, alias: u64) -> AliasGuard {
        todo!()
    }

    pub async fn find_alias(&self, alias: u64) -> anyhow::Result<Option<PeerAddress>> {
        todo!()
    }

    pub async fn unicast_without_ack(&self, dest: PeerAddress, data: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    pub async fn broadcast_without_ack(&self, data: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    pub async fn open_stream(&self, dest: PeerAddress, meta: Vec<u8>) -> anyhow::Result<P2pQuicStream> {
        todo!()
    }

    pub async fn recv(&mut self) -> Option<P2pServiceEvent> {
        self.rx.recv().await
    }
}

impl P2pServiceRequester {
    pub fn register_alias(&self, alias: u64) -> AliasGuard {
        todo!()
    }

    pub async fn find_alias(&self, alias: u64) -> anyhow::Result<Option<PeerAddress>> {
        todo!()
    }

    pub async fn unicast_without_ack(&self, dest: PeerAddress, data: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    pub async fn broadcast_without_ack(&self, data: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    pub async fn open_stream(&self, dest: PeerAddress, meta: Vec<u8>) -> anyhow::Result<P2pQuicStream> {
        todo!()
    }
}

impl P2pServiceBuilder {
    pub fn add(&mut self, service_id: u8) -> P2pService {
        let (tx, rx) = channel(SERVICE_CHANNEL_SIZE);
        self.services[service_id as usize] = Some(tx);
        P2pService { rx }
    }

    pub fn build(self) -> [Option<Sender<P2pServiceEvent>>; 256] {
        self.services
    }
}
