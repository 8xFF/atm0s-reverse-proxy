use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};

use super::{AgentListener, AgentListenerEvent};

pub type TunnelTcpStream = TcpStream;
pub struct AgentTcpListener {
    listener: TcpListener,
}

impl AgentTcpListener {
    pub async fn new(addr: SocketAddr) -> anyhow::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr).await?,
        })
    }
}

impl AgentListener<TunnelTcpStream> for AgentTcpListener {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<TunnelTcpStream>> {
        let _stream = self.listener.accept().await?;
        todo!()
    }
}
