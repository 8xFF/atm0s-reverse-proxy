use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

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

pub async fn wait_object<
    R: AsyncRead + Send + Unpin,
    O: DeserializeOwned,
    const MAX_SIZE: usize,
>(
    reader: &mut R,
) -> Result<O, String> {
    let mut len_buf = [0; 2];
    let mut data_buf = [0; MAX_SIZE];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| e.to_string())?;
    let handshake_len = u16::from_be_bytes([len_buf[0], len_buf[1]]) as usize;
    if handshake_len > data_buf.len() {
        return Err("Package too big".to_string());
    }

    reader
        .read_exact(&mut data_buf[0..handshake_len])
        .await
        .map_err(|e| e.to_string())?;

    bincode::deserialize(&data_buf[0..handshake_len]).map_err(|e| e.to_string())
}

pub async fn write_object<W: AsyncWrite + Send + Unpin, O: Serialize, const MAX_SIZE: usize>(
    writer: &mut W,
    object: O,
) -> Result<(), String> {
    let data_buf: Vec<u8> = bincode::serialize(&object).expect("Should convert to binary");
    if data_buf.len() > MAX_SIZE {
        return Err("Buffer to big".to_string());
    }
    let len_buf = (data_buf.len() as u16).to_be_bytes();

    writer
        .write_all(&len_buf)
        .await
        .map_err(|e| e.to_string())?;
    writer
        .write_all(&data_buf)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
