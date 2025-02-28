//! Tunnel is a trait that defines the interface for a tunnel which connect to connector port of relayer.

use std::{fmt::Debug, str::FromStr};

use protocol::stream::TunnelStream;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(feature = "quic")]
pub mod quic;

#[cfg(feature = "tcp")]
pub mod tcp;

#[cfg(feature = "tls")]
pub mod tls;

#[derive(Debug, Clone)]
pub enum Protocol {
    #[cfg(feature = "tcp")]
    Tcp,
    #[cfg(feature = "tls")]
    Tls,
    #[cfg(feature = "quic")]
    Quic,
}

impl FromStr for Protocol {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "tcp")]
            "tcp" | "TCP" => Ok(Protocol::Tcp),
            #[cfg(feature = "tls")]
            "tls" | "TLS" => Ok(Protocol::Tls),
            #[cfg(feature = "quic")]
            "quic" | "QUIC" => Ok(Protocol::Quic),
            _ => Err("invalid protocol"),
        }
    }
}

pub trait SubConnection: AsyncRead + AsyncWrite + Unpin + Send + Sync {}

impl<R: AsyncRead + Unpin + Send + Sync, W: AsyncWrite + Unpin + Send + Sync> SubConnection for TunnelStream<R, W> {}

#[async_trait::async_trait]
pub trait Connection<S: SubConnection>: Send + Sync {
    async fn create_outgoing(&mut self) -> anyhow::Result<S>;
    async fn recv(&mut self) -> anyhow::Result<S>;
}
