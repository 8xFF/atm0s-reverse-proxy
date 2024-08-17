use std::net::SocketAddr;

use futures::{AsyncRead, AsyncWrite};

pub mod registry;
pub mod tcp;

pub trait LocalTunnel<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    fn split(self) -> (R, W);
}

pub trait ServiceRegistry {
    fn dest_for(&self, tls: bool, service: Option<u16>, domain: &str) -> Option<SocketAddr>;
}
