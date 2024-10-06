use derive_more::derive::{Deref, Display, From};
use p2p::AliasGuard;
use protocol::AgentId;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc::Sender, oneshot},
};

pub mod quic;
pub mod tcp;

#[derive(Debug, Hash, Display, PartialEq, Eq, From, Deref, Clone, Copy)]
pub struct AgentSessionId(u64);

impl AgentSessionId {
    pub fn rand() -> Self {
        Self(rand::random())
    }
}

enum AgentSessionControl<S> {
    CreateStream(oneshot::Sender<anyhow::Result<S>>),
}

#[derive(Debug)]
pub struct AgentSession<S> {
    agent_id: AgentId,
    session_id: AgentSessionId,
    control_tx: Sender<AgentSessionControl<S>>,
    alias_guard: Option<AliasGuard>,
}

impl<S> AgentSession<S> {
    fn new(agent_id: AgentId, session_id: AgentSessionId, control_tx: Sender<AgentSessionControl<S>>) -> Self {
        Self {
            agent_id,
            session_id,
            control_tx,
            alias_guard: None,
        }
    }

    /// We keep this alias guard here fore automatic unregister when session destroyed
    pub(super) fn set_alias_guard(&mut self, alias: AliasGuard) {
        self.alias_guard = Some(alias);
    }
}

impl<S> Clone for AgentSession<S> {
    fn clone(&self) -> Self {
        Self {
            agent_id: self.agent_id,
            session_id: self.session_id,
            control_tx: self.control_tx.clone(),
            alias_guard: self.alias_guard.clone(),
        }
    }
}

pub enum AgentListenerEvent<S> {
    Connected(AgentId, AgentSession<S>),
    IncomingStream(AgentId, S),
    Disconnected(AgentId, AgentSessionId),
}

pub trait AgentListener<S: AsyncRead + AsyncWrite> {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<S>>;
    async fn shutdown(&mut self);
}

impl<S: AsyncRead + AsyncWrite + Send + Sync + 'static> AgentSession<S> {
    pub fn session_id(&self) -> AgentSessionId {
        self.session_id
    }

    pub async fn create_stream(&self) -> anyhow::Result<S> {
        let (tx, rx) = oneshot::channel();
        self.control_tx.send(AgentSessionControl::CreateStream(tx)).await?;
        Ok(rx.await??)
    }
}
