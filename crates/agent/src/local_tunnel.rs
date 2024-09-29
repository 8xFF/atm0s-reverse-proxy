use std::net::SocketAddr;

use futures::{AsyncRead, AsyncWrite};

pub mod registry;
pub mod tcp;

pub trait LocalTunnel: AsyncRead + AsyncWrite + Unpin + Send + Sync {}

pub trait ServiceRegistry {
    fn dest_for(&self, tls: bool, service: Option<u16>, domain: &str) -> Option<SocketAddr>;
}
