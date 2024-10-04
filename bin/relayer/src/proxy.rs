use std::{net::SocketAddr, sync::Arc};

use anyhow::Ok;
use protocol::stream::TunnelStream;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
};

use crate::agent::AgentId;

pub mod http;
pub mod rtsp;
pub mod tls;

pub struct ProxyDestination {
    pub domain: String,
    pub service: u16,
}

impl ProxyDestination {
    pub fn agent_id(&self) -> AgentId {
        todo!()
    }
}

pub trait ProxyDestinationDetector {
    async fn determine<S: AsyncRead + AsyncWrite>(
        &self,
        stream: &mut S,
    ) -> anyhow::Result<ProxyDestination>;
}

pub struct ProxyTcpListener<Detector> {
    listener: TcpListener,
    detector: Arc<Detector>,
}

impl<Detector: ProxyDestinationDetector> ProxyTcpListener<Detector> {
    pub async fn new(addr: SocketAddr, detector: Detector) -> anyhow::Result<Self> {
        Ok(Self {
            detector: detector.into(),
            listener: TcpListener::bind(addr).await?,
        })
    }

    pub async fn recv(&mut self) -> anyhow::Result<(ProxyDestination, TcpStream)> {
        let (mut stream, _remote) = self.listener.accept().await?;
        let dest = self.detector.determine(&mut stream).await?;
        Ok((dest, stream))
    }
}
