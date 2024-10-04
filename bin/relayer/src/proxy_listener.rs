//! Proxy is a server which accept connections from users and forward them to the target agent.
use std::pin::Pin;

use derive_more::derive::From;
use tcp::ProxyTcpTunnel;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::ProxyClusterIncomingTunnel;

pub mod cluster;
pub mod tcp;

pub trait DomainDetector: Send + Sync {
    fn name(&self) -> &str;
    fn get_domain(&self, buf: &[u8]) -> Option<String>;
}

pub trait ProxyTunnel: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static {
    fn wait(&mut self) -> impl std::future::Future<Output = Option<()>> + Send;
    fn source_addr(&self) -> String;
    fn local(&self) -> bool;
    fn domain(&self) -> &str;
    fn handshake(&self) -> &[u8];
}

pub trait ProxyListener: Send + Sync {
    type Stream;
    fn recv(&mut self) -> impl std::future::Future<Output = Option<Self::Stream>> + Send;
}

#[derive(From)]
pub enum ProxyTunnelWrap {
    Tcp(ProxyTcpTunnel),
    Cluster(ProxyClusterIncomingTunnel),
}

impl ProxyTunnel for ProxyTunnelWrap {
    async fn wait(&mut self) -> Option<()> {
        match self {
            ProxyTunnelWrap::Tcp(inner) => inner.wait().await,
            ProxyTunnelWrap::Cluster(inner) => inner.wait().await,
        }
    }

    fn source_addr(&self) -> String {
        match self {
            ProxyTunnelWrap::Tcp(inner) => inner.source_addr(),
            ProxyTunnelWrap::Cluster(inner) => inner.source_addr(),
        }
    }

    fn local(&self) -> bool {
        match self {
            ProxyTunnelWrap::Tcp(inner) => inner.local(),
            ProxyTunnelWrap::Cluster(inner) => inner.local(),
        }
    }

    fn domain(&self) -> &str {
        match self {
            ProxyTunnelWrap::Tcp(inner) => inner.domain(),
            ProxyTunnelWrap::Cluster(inner) => inner.domain(),
        }
    }

    fn handshake(&self) -> &[u8] {
        match self {
            ProxyTunnelWrap::Tcp(inner) => inner.handshake(),
            ProxyTunnelWrap::Cluster(inner) => inner.handshake(),
        }
    }
}

impl AsyncRead for ProxyTunnelWrap {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ProxyTunnelWrap::Tcp(inner) => Pin::new(inner).poll_read(cx, buf),
            ProxyTunnelWrap::Cluster(inner) => Pin::new(inner).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ProxyTunnelWrap {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            ProxyTunnelWrap::Tcp(inner) => Pin::new(inner).poll_write(cx, buf),
            ProxyTunnelWrap::Cluster(inner) => Pin::new(inner).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            ProxyTunnelWrap::Tcp(inner) => Pin::new(inner).poll_flush(cx),
            ProxyTunnelWrap::Cluster(inner) => Pin::new(inner).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            ProxyTunnelWrap::Tcp(inner) => Pin::new(inner).poll_shutdown(cx),
            ProxyTunnelWrap::Cluster(inner) => Pin::new(inner).poll_shutdown(cx),
        }
    }
}
