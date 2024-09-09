use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterTunnelRequest {
    pub domain: String,
    pub handshake: Vec<u8>,
}

impl From<&ClusterTunnelRequest> for Vec<u8> {
    fn from(resp: &ClusterTunnelRequest) -> Self {
        bincode::serialize(resp).expect("Should ok")
    }
}

impl TryFrom<&[u8]> for ClusterTunnelRequest {
    type Error = bincode::Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(buf)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterTunnelResponse {
    pub success: bool,
}

impl From<&ClusterTunnelResponse> for Vec<u8> {
    fn from(resp: &ClusterTunnelResponse) -> Self {
        bincode::serialize(resp).expect("Should ok")
    }
}

impl TryFrom<&[u8]> for ClusterTunnelResponse {
    type Error = bincode::Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(buf)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentTunnelRequest {
    pub service: Option<u16>,
    pub tls: bool,
    pub domain: String,
}

impl From<&AgentTunnelRequest> for Vec<u8> {
    fn from(resp: &AgentTunnelRequest) -> Self {
        bincode::serialize(resp).expect("Should ok")
    }
}

impl TryFrom<&[u8]> for AgentTunnelRequest {
    type Error = bincode::Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(buf)
    }
}
