use std::marker::PhantomData;

use derive_more::derive::{Deref, From};
use tokio::io::{AsyncRead, AsyncWrite};

pub mod quic;
pub mod tcp;

#[derive(Debug, Hash, PartialEq, Eq, From, Deref)]
pub struct AgentId(u64);

#[derive(Debug, Hash, PartialEq, Eq, From, Deref)]
pub struct AgentSessionId(u64);

#[derive(Debug)]
pub struct AgentSession<S> {
    _tmp: PhantomData<S>,
}

impl<S> Clone for AgentSession<S> {
    fn clone(&self) -> Self {
        Self {
            _tmp: self._tmp.clone(),
        }
    }
}

pub enum AgentListenerEvent<S> {
    Connected(AgentId, AgentSession<S>),
    Disconnected(AgentId, AgentSessionId),
}

pub trait AgentListener<S: AsyncRead + AsyncWrite> {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<S>>;
}

impl<S: AsyncRead + AsyncWrite> AgentSession<S> {
    pub fn session_id(&self) -> AgentSessionId {
        todo!()
    }

    pub async fn create_stream(&self) -> anyhow::Result<S> {
        todo!()
    }
}
