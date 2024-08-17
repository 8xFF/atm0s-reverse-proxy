//! Proxy is a server which accept connections from users and forward them to the target agent.

use futures::{AsyncRead, AsyncWrite};

pub mod cluster;
pub mod tcp;

pub trait DomainDetector: Send + Sync {
    fn get_domain(&self, buf: &[u8]) -> Option<String>;
}

#[async_trait::async_trait]
pub trait ProxyTunnel: Send + Sync {
    async fn wait(&mut self) -> Option<()>;
    fn local(&self) -> bool;
    fn domain(&self) -> &str;
    fn handshake(&self) -> Option<&[u8]>;
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
