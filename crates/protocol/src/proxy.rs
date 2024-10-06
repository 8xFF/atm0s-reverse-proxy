use std::fmt::Display;

use derive_more::derive::{Deref, From};
use serde::{Deserialize, Serialize};

#[derive(Debug, Hash, PartialEq, Eq, From, Deref, Clone, Copy)]
pub struct AgentId(u64);

impl Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("AgentId({:02x})", self.0))
    }
}

impl AgentId {
    pub fn from_domain(domain: &str) -> Self {
        Self(u64::from_be_bytes(domain.as_bytes()[0..8].try_into().expect("should convert to u64")))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyDestination {
    pub domain: String,
    pub service: Option<u16>,
    pub tls: bool,
}

impl ProxyDestination {
    pub fn agent_id(&self) -> AgentId {
        AgentId::from_domain(&self.domain)
    }
}
