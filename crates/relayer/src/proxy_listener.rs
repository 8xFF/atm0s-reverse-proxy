//! Proxy is a server which accept connections from users and forward them to the target agent.

use futures::{AsyncRead, AsyncWrite};

pub mod http;

#[async_trait::async_trait]
pub trait ProxyTunnel <R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    fn first_pkt(&self) -> &[u8];
    async fn wait(&mut self) -> Option<()>;
    fn domain(&self) -> &str;
    fn split (self) -> (R, W);
}

#[async_trait::async_trait]
pub trait ProxyListener<S: ProxyTunnel<R, W>, R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    async fn recv(&mut self) -> Option<S>;
}