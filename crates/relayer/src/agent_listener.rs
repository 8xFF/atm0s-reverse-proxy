//! Connector is server which accept connection from agent and wait msg from user.

use std::{error::Error, sync::Arc};

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
    async fn create_sub_connection(&mut self) -> Result<S, Box<dyn Error>>;
    async fn recv(&mut self, rpc_handler: &Arc<dyn AgentRpcHandler>) -> Result<(), Box<dyn Error>>;
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
pub trait AgentRpcHandler: Send + Sync {
    async fn handle(&self, agent_domain: &str, req: &[u8]) -> Vec<u8>;
}

#[derive(Default)]
pub struct AgentRpcHandlerDummy {}

#[async_trait::async_trait]
impl AgentRpcHandler for AgentRpcHandlerDummy {
    async fn handle(&self, _agent_domain: &str, _req: &[u8]) -> Vec<u8> {
        vec![]
    }
}
