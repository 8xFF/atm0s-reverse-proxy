//! Tunnel is a trait that defines the interface for a tunnel which connect to connector port of relayer.

use std::fmt::Debug;

use clap::ValueEnum;
use protocol::stream::TunnelStream;
use tokio::io::{AsyncRead, AsyncWrite};

pub mod quic;
pub mod tcp;

#[derive(ValueEnum, Debug, Clone)]
pub enum Protocol {
    Tcp,
    Quic,
}

pub trait SubConnection: AsyncRead + AsyncWrite + Unpin + Send + Sync {}

impl<R: AsyncRead + Unpin + Send + Sync, W: AsyncWrite + Unpin + Send + Sync> SubConnection for TunnelStream<R, W> {}

#[async_trait::async_trait]
pub trait Connection<S: SubConnection>: Send + Sync {
    async fn create_outgoing(&mut self) -> anyhow::Result<S>;
    async fn recv(&mut self) -> anyhow::Result<S>;
}
