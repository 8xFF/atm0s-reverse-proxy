use std::{net::SocketAddr, sync::Arc};

use anyhow::Ok;
use protocol::proxy::ProxyDestination;
use tokio::net::{TcpListener, TcpStream};

pub mod http;
pub mod rtsp;
pub mod tls;

pub trait ProxyDestinationDetector {
    async fn determine(&self, stream: &mut TcpStream) -> anyhow::Result<ProxyDestination>;
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
