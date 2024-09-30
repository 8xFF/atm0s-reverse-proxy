use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use protocol::cluster::AgentTunnelRequest;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
};

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

impl ProxyListener for ProxyTcpListener {
    type Stream = ProxyTcpTunnel;
    async fn recv(&mut self) -> Option<Self::Stream> {
        let (stream, remote) = self.tcp_listener.accept().await.ok()?;
        log::info!("[ProxyTcpListener] new conn from {}", remote);
        Some(ProxyTcpTunnel {
            stream_addr: remote,
            detector: self.detector.clone(),
            service: self.service,
            domain: "".to_string(),
            handshake: vec![],
            stream,
            tls: self.tls,
        })
    }
}

pub struct ProxyTcpTunnel {
    stream_addr: SocketAddr,
    detector: Arc<dyn DomainDetector>,
    service: Option<u16>,
    domain: String,
    stream: TcpStream,
    handshake: Vec<u8>,
    tls: bool,
}

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
        let first_pkt_size = self.stream.peek(&mut first_pkt).await.ok()?;
        log::info!(
            "[ProxyTcpTunnel] read {} bytes for determine url",
            first_pkt_size
        );
        if first_pkt_size == 0 {
            log::warn!("[ProxyTcpTunnel] connect close without data");
            return None;
        }
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
}

impl AsyncRead for ProxyTcpTunnel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProxyTcpTunnel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}
