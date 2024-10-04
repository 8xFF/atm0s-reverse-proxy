use std::net::SocketAddr;

use tokio::net::TcpStream;

use super::{AgentListener, AgentListenerEvent};

pub type TunnelTcpStream = TcpStream;
pub struct AgentTcpListener {}

impl AgentTcpListener {
    pub async fn new(addr: SocketAddr) -> anyhow::Result<Self> {
        Ok(Self {})
    }
}

impl AgentListener<TunnelTcpStream> for AgentTcpListener {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<TunnelTcpStream>> {
        todo!()
    }
}
