use std::net::SocketAddr;

use quinn::Endpoint;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

use crate::quic::{make_server_endpoint, TunnelQuicStream};

use super::{AgentListener, AgentListenerEvent};

pub struct AgentQuicListener {
    endpoint: Endpoint,
}

impl AgentQuicListener {
    pub async fn new(
        addr: SocketAddr,
        priv_key: PrivatePkcs8KeyDer<'static>,
        cert: CertificateDer<'static>,
    ) -> anyhow::Result<Self> {
        let endpoint = make_server_endpoint(addr, priv_key, cert)?;

        Ok(Self { endpoint })
    }
}

impl AgentListener<TunnelQuicStream> for AgentQuicListener {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<TunnelQuicStream>> {
        todo!()
    }
}
