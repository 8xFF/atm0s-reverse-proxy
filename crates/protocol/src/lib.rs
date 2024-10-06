use std::fmt::Display;

use derive_more::derive::{Deref, From};

pub mod cluster;
pub mod key;
pub mod services;
pub mod stream;
pub mod time;

pub const DEFAULT_TUNNEL_CERT: &[u8] = include_bytes!("../certs/tunnel.cert");
pub const DEFAULT_TUNNEL_KEY: &[u8] = include_bytes!("../certs/tunnel.key");

pub const DEFAULT_CLUSTER_CERT: &[u8] = include_bytes!("../certs/cluster.cert");
pub const DEFAULT_CLUSTER_KEY: &[u8] = include_bytes!("../certs/cluster.key");

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
