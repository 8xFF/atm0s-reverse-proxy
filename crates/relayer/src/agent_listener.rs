//! Connector is server which accept connection from agent and wait msg from user.

use std::error::Error;

use futures::{AsyncRead, AsyncWrite};
use protocol::stream::NamedStream;

pub mod quic;
pub mod tcp;

pub trait AgentSubConnection:
    AsyncRead + AsyncWrite + NamedStream + Send + Sync + Unpin + 'static
{
}

pub trait AgentConnection<S: AgentSubConnection> {
    fn domain(&self) -> String;
    fn create_sub_connection(
        &mut self,
    ) -> impl std::future::Future<Output = Result<S, Box<dyn Error>>> + Send;
    fn recv(&mut self) -> impl std::future::Future<Output = Result<S, Box<dyn Error>>> + Send;
}

pub trait AgentListener<C: AgentConnection<S>, S: AgentSubConnection> {
    fn recv(&mut self) -> impl std::future::Future<Output = Result<C, Box<dyn Error>>> + Send;
}

pub trait AgentConnectionHandler<S: AgentSubConnection + Send + Sync>:
    Send + Sync + Clone + 'static
{
    fn handle(
        &self,
        agent_domain: &str,
        connection: S,
    ) -> impl std::future::Future<Output = Result<(), Box<dyn Error>>> + Send;
}

pub struct AgentIncomingConnHandlerDummy<S: AgentSubConnection> {
    _phantom: std::marker::PhantomData<S>,
}

impl<S: AgentSubConnection> Clone for AgentIncomingConnHandlerDummy<S> {
    fn clone(&self) -> Self {
        Self {
            _phantom: self._phantom.clone(),
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
    async fn handle(&self, agent_domain: &str, _connection: S) -> Result<(), Box<dyn Error>> {
        log::info!("on connection from agent {}", agent_domain);
        Ok(())
    }
}
