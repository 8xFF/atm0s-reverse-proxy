//! Tunnel is a trait that defines the interface for a tunnel which connect to connector port of relayer.

use std::error::Error;

use clap::ValueEnum;
use futures::{AsyncRead, AsyncWrite};

pub mod quic;
pub mod tcp;

#[derive(ValueEnum, Debug, Clone)]
pub enum Protocol {
    Tcp,
    Quic,
}

pub trait SubConnection<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    fn split(self) -> (R, W);
}

#[async_trait::async_trait]
pub trait Connection<S: SubConnection<R, W>, R: AsyncRead + Unpin, W: AsyncWrite + Unpin>:
    Send + Sync
{
    async fn recv(&mut self) -> Result<S, Box<dyn Error>>;
}
