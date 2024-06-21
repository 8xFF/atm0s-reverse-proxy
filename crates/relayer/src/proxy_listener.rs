//! Proxy is a server which accept connections from users and forward them to the target agent.

use futures::{AsyncRead, AsyncWrite};

pub mod cluster;
pub mod http;

#[async_trait::async_trait]
pub trait ProxyTunnel: Send + Sync {
    async fn wait(&mut self) -> Option<()>;
    fn local(&self) -> bool;
    fn domain(&self) -> &str;
    fn split(
        &mut self,
    ) -> (
        Box<dyn AsyncRead + Send + Sync + Unpin>,
        Box<dyn AsyncWrite + Send + Sync + Unpin>,
    );
}

#[async_trait::async_trait]
pub trait ProxyListener: Send + Sync {
    async fn recv(&mut self) -> Option<Box<dyn ProxyTunnel>>;
}
