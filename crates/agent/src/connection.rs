//! Tunnel is a trait that defines the interface for a tunnel which connect to connector port of relayer.

use futures::{AsyncRead, AsyncWrite};

pub mod tcp;

pub trait SubConnection<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    fn split (self) -> (R, W);
}

#[async_trait::async_trait]
pub trait Connection<S: SubConnection<R, W>, R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    async fn recv(&mut self) -> Option<S>;
}