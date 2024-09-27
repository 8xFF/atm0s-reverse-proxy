//! Connector is server which accept connection from agent and wait msg from user.

use std::error::Error;

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
    fn conn_id(&self) -> u64;
    async fn create_sub_connection(&mut self) -> Result<S, Box<dyn Error>>;
    async fn recv(&mut self) -> Result<S, Box<dyn Error>>;
}

#[async_trait::async_trait]
pub trait AgentListener<
    C: AgentConnection<S, R, W>,
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
>: Send + Sync
{
    async fn recv(&mut self) -> Result<C, Box<dyn Error>>;
}

#[async_trait::async_trait]
pub trait AgentConnectionHandler<
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
>: Send + Sync
{
    async fn handle(&self, agent_domain: &str, connection: S) -> Result<(), Box<dyn Error>>;
}

pub struct AgentIncomingConnHandlerDummy<
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Send + Sync + Unpin,
    W: AsyncWrite + Send + Sync + Unpin,
> {
    _phantom: std::marker::PhantomData<(S, R, W)>,
}

impl<
        S: AgentSubConnection<R, W>,
        R: AsyncRead + Send + Sync + Unpin,
        W: AsyncWrite + Send + Sync + Unpin,
    > Default for AgentIncomingConnHandlerDummy<S, R, W>
{
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<
        S: AgentSubConnection<R, W>,
        R: AsyncRead + Send + Sync + Unpin,
        W: AsyncWrite + Send + Sync + Unpin,
    > AgentConnectionHandler<S, R, W> for AgentIncomingConnHandlerDummy<S, R, W>
{
    async fn handle(&self, agent_domain: &str, _connection: S) -> Result<(), Box<dyn Error>> {
        log::info!("on connection from agent {}", agent_domain);
        Ok(())
    }
}
