pub mod bincode_stream;
pub mod cluster;
pub mod key;
pub mod services;
pub mod stream;
pub mod time;

pub const DEFAULT_TUNNEL_CERT: &[u8] = include_bytes!("../certs/tunnel.cert");
pub const DEFAULT_TUNNEL_KEY: &[u8] = include_bytes!("../certs/tunnel.key");

pub const DEFAULT_CLUSTER_CERT: &[u8] = include_bytes!("../certs/cluster.cert");
pub const DEFAULT_CLUSTER_KEY: &[u8] = include_bytes!("../certs/cluster.key");
