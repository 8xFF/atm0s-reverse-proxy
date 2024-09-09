use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use async_std::net::{TcpListener, TcpStream};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite};
use protocol::cluster::AgentTunnelRequest;

use super::{DomainDetector, ProxyListener, ProxyTunnel};

mod http_detector;
mod rtsp_detector;
mod tls_detector;

pub use http_detector::*;
pub use rtsp_detector::*;
pub use tls_detector::*;

pub struct ProxyTcpListener {
    tcp_listener: TcpListener,
    tls: bool,
    service: Option<u16>,
    detector: Arc<dyn DomainDetector>,
}

impl ProxyTcpListener {
    pub async fn new(
        port: u16,
        tls: bool,
        service: Option<u16>,
        detector: Arc<dyn DomainDetector>,
    ) -> Option<Self> {
        log::info!(
            "ProxyTcpListener::new port {port} tls {tls} service {:?}",
            service
        );
        Some(Self {
            tcp_listener: TcpListener::bind(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port))
                .await
                .ok()?,
            tls,
            service,
            detector,
        })
    }
}

#[async_trait::async_trait]
impl ProxyListener for ProxyTcpListener {
    async fn recv(&mut self) -> Option<Box<dyn ProxyTunnel>> {
        let (stream, remote) = self.tcp_listener.accept().await.ok()?;
        log::info!("[ProxyTcpListener] new conn from {}", remote);
        Some(Box::new(ProxyTcpTunnel {
            stream_addr: remote,
            detector: self.detector.clone(),
            service: self.service,
            domain: "".to_string(),
            handshake: vec![],
            stream: Some(stream),
            tls: self.tls,
        }))
    }
}

pub struct ProxyTcpTunnel {
    stream_addr: SocketAddr,
    detector: Arc<dyn DomainDetector>,
    service: Option<u16>,
    domain: String,
    stream: Option<TcpStream>,
    handshake: Vec<u8>,
    tls: bool,
}

#[async_trait::async_trait]
impl ProxyTunnel for ProxyTcpTunnel {
    fn source_addr(&self) -> String {
        if self.tls {
            format!("tls+{}://{}", self.detector.name(), self.stream_addr)
        } else {
            format!("tcp+{}://{}", self.detector.name(), self.stream_addr)
        }
    }

    async fn wait(&mut self) -> Option<()> {
        log::info!("[ProxyTcpTunnel] wait first data for checking url...");
        let mut first_pkt = [0u8; 4096];
        let stream = self.stream.as_mut()?;
        let first_pkt_size = stream.peek(&mut first_pkt).await.ok()?;
        log::info!(
            "[ProxyTcpTunnel] read {} bytes for determine url",
            first_pkt_size
        );
        self.domain = self.detector.get_domain(&first_pkt[..first_pkt_size])?;
        log::info!("[ProxyTcpTunnel] detected domain {}", self.domain);
        self.handshake = (&AgentTunnelRequest {
            service: self.service,
            tls: self.tls,
            domain: self.domain.clone(),
        })
            .into();
        Some(())
    }

    fn local(&self) -> bool {
        true
    }

    fn domain(&self) -> &str {
        &self.domain
    }

    fn handshake(&self) -> &[u8] {
        &self.handshake
    }

    fn split(
        &mut self,
    ) -> (
        Box<dyn AsyncRead + Send + Sync + Unpin>,
        Box<dyn AsyncWrite + Send + Sync + Unpin>,
    ) {
        let (read, write) = AsyncReadExt::split(self.stream.take().expect("Should has stream"));
        (Box::new(read), Box::new(write))
    }
}
