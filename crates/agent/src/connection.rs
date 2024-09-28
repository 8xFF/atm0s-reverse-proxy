//! Tunnel is a trait that defines the interface for a tunnel which connect to connector port of relayer.

use std::error::Error;

use clap::ValueEnum;
use futures::{AsyncRead, AsyncWrite};
use protocol::stream::NamedStream;

pub mod quic;
pub mod tcp;

#[derive(ValueEnum, Debug, Clone)]
pub enum Protocol {
    Tcp,
    Quic,
}

pub trait SubConnection: AsyncRead + AsyncWrite + NamedStream + Unpin + Send + Sync {}

#[async_trait::async_trait]
pub trait Connection<S: SubConnection>: Send + Sync {
    async fn create_outgoing(&mut self) -> Result<S, Box<dyn Error>>;
    async fn recv(&mut self) -> Result<S, Box<dyn Error>>;
}
