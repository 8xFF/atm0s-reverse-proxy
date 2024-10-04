//! Connector is server which accept connection from agent and wait msg from user.

use tokio::io::{AsyncRead, AsyncWrite};

pub mod quic;
pub mod tcp;

pub trait AgentSubConnection: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static {}

pub trait AgentConnection<S: AgentSubConnection> {
    fn conn_id(&self) -> u64;
    fn domain(&self) -> String;
    fn create_sub_connection(
        &mut self,
    ) -> impl std::future::Future<Output = anyhow::Result<S>> + Send;
    fn recv(&mut self) -> impl std::future::Future<Output = anyhow::Result<S>> + Send;
}

pub trait AgentListener<C: AgentConnection<S>, S: AgentSubConnection> {
    fn recv(&mut self) -> impl std::future::Future<Output = anyhow::Result<C>> + Send;
}

pub trait AgentConnectionHandler<S: AgentSubConnection + Send + Sync>:
    Send + Sync + Clone + 'static
{
    fn name(&self) -> &str;
    fn handle(
        &self,
        agent_domain: &str,
        connection: S,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}

pub struct AgentIncomingConnHandlerDummy<S: AgentSubConnection> {
    _phantom: std::marker::PhantomData<S>,
}

impl<S: AgentSubConnection> Clone for AgentIncomingConnHandlerDummy<S> {
    fn clone(&self) -> Self {
        Self {
            _phantom: self._phantom,
        }
    }
}

impl<S: AgentSubConnection> Default for AgentIncomingConnHandlerDummy<S> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S: AgentSubConnection> AgentConnectionHandler<S> for AgentIncomingConnHandlerDummy<S> {
    fn name(&self) -> &str {
        "dummy"
    }

    async fn handle(&self, agent_domain: &str, _connection: S) -> anyhow::Result<()> {
        log::info!("on connection from agent {}", agent_domain);
        Ok(())
    }
}
