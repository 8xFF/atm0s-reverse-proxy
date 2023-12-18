//! Connector is server which accept connection from agent and wait msg from user.

use futures::{AsyncRead, AsyncWrite};

pub mod quic;
pub mod tcp;

pub trait AgentSubConnection<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    fn split(self) -> (R, W);
}

#[async_trait::async_trait]
pub trait AgentConnection<S: AgentSubConnection<R, W>, R: AsyncRead + Unpin, W: AsyncWrite + Unpin>:
    Send + Sync
{
    fn domain(&self) -> String;
    async fn create_sub_connection(&mut self) -> Option<S>;
    async fn recv(&mut self) -> Option<()>;
}

#[async_trait::async_trait]
pub trait AgentListener<
    C: AgentConnection<S, R, W>,
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
>: Send + Sync
{
    async fn recv(&mut self) -> Option<C>;
}
