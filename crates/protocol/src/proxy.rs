use std::fmt::Display;

use anyhow::anyhow;
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
    pub fn try_from_domain(domain: &str) -> anyhow::Result<Self> {
        let (first, _) = domain.as_bytes().split_at_checked(8).ok_or(anyhow!("domain should be at least 8 bytes"))?;
        Ok(Self(u64::from_be_bytes(first.try_into()?)))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyDestination {
    pub domain: String,
    pub service: Option<u16>,
    pub tls: bool,
}

impl ProxyDestination {
    pub fn agent_id(&self) -> anyhow::Result<AgentId> {
        AgentId::try_from_domain(&self.domain)
    }
}
