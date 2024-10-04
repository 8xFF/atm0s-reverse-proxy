use std::{collections::HashMap, net::SocketAddr};

use alias::AliasGuard;
use derive_more::derive::{Deref, Display, From};
use peer::{ConnectionId, PeerConnection};
use protocol::stream::TunnelStream;
use quinn::{Endpoint, RecvStream, SendStream};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

use crate::quic::make_server_endpoint;

mod alias;
mod peer;

#[derive(Debug, Display, Clone, Copy, From, Deref)]
pub struct NodeId(u32);

pub struct UnstrutedSdnRequester {}

pub struct UnstructedSdn {
    endpoint: Endpoint,
    peer_conns: HashMap<ConnectionId, PeerConnection>,
}

impl UnstructedSdn {
    pub async fn new(node: NodeId, listener: SocketAddr, priv_key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>) -> anyhow::Result<Self> {
        let endpoint = make_server_endpoint(listener, priv_key, cert)?;

        Ok(Self { endpoint, peer_conns: HashMap::new() })
    }

    pub fn requester(&mut self) -> UnstrutedSdnRequester {
        todo!()
    }

    pub async fn recv(&mut self) -> anyhow::Result<()> {
        loop {
            if let Some(connecting) = self.endpoint.accept().await {
                let peer_conn = PeerConnection::new(connecting);
                self.peer_conns.insert(peer_conn.conn_id(), peer_conn);
            }
        }
    }
}

impl UnstrutedSdnRequester {
    pub async fn register_alias(&self, alias: u64) -> AliasGuard {
        todo!()
    }

    pub async fn find_alias(&self, alias: u64) -> anyhow::Result<Option<NodeId>> {
        todo!()
    }

    pub async fn create_stream_to(&self, dest: NodeId) -> anyhow::Result<TunnelStream<RecvStream, SendStream>> {
        todo!()
    }
}
