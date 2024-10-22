use anyhow::anyhow;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterTunnelRequest {
    pub domain: String,
    pub handshake: Vec<u8>,
}

impl From<&ClusterTunnelRequest> for Vec<u8> {
    fn from(resp: &ClusterTunnelRequest) -> Self {
        bincode::serialize(resp).expect("Should serialize cluster tunnel request")
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
        bincode::serialize(resp).expect("Should serialize cluster tunnel response")
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
        bincode::serialize(resp).expect("Should serialize agent tunnel request")
    }
}

impl TryFrom<&[u8]> for AgentTunnelRequest {
    type Error = bincode::Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(buf)
    }
}

pub async fn wait_object<R: AsyncRead + Unpin, O: DeserializeOwned, const MAX_SIZE: usize>(reader: &mut R) -> anyhow::Result<O> {
    let mut len_buf = [0; 2];
    let mut data_buf = [0; MAX_SIZE];
    reader.read_exact(&mut len_buf).await?;
    let handshake_len = u16::from_be_bytes([len_buf[0], len_buf[1]]) as usize;
    if handshake_len > data_buf.len() {
        return Err(anyhow!("packet to big {} vs {MAX_SIZE}", data_buf.len()));
    }

    reader.read_exact(&mut data_buf[0..handshake_len]).await?;

    Ok(bincode::deserialize(&data_buf[0..handshake_len])?)
}

pub async fn write_object<W: AsyncWrite + Send + Unpin, O: Serialize, const MAX_SIZE: usize>(writer: &mut W, object: &O) -> anyhow::Result<()> {
    let data_buf: Vec<u8> = bincode::serialize(object).expect("Should convert to binary");
    if data_buf.len() > MAX_SIZE {
        return Err(anyhow!("buffer to big {} vs {MAX_SIZE}", data_buf.len()));
    }
    let len_buf = (data_buf.len() as u16).to_be_bytes();

    writer.write_all(&len_buf).await?;
    writer.write_all(&data_buf).await?;
    Ok(())
}

pub async fn wait_buf<R: AsyncRead + Unpin, const MAX_SIZE: usize>(reader: &mut R) -> anyhow::Result<Vec<u8>> {
    let mut len_buf = [0; 2];
    let mut data_buf = [0; MAX_SIZE];
    reader.read_exact(&mut len_buf).await?;
    let handshake_len = u16::from_be_bytes([len_buf[0], len_buf[1]]) as usize;
    if handshake_len > data_buf.len() {
        return Err(anyhow!("packet to big {} vs {MAX_SIZE}", data_buf.len()));
    }

    reader.read_exact(&mut data_buf[0..handshake_len]).await?;

    Ok(data_buf[0..handshake_len].to_vec())
}

pub async fn write_buf<W: AsyncWrite + Send + Unpin, const MAX_SIZE: usize>(writer: &mut W, data_buf: &[u8]) -> anyhow::Result<()> {
    if data_buf.len() > MAX_SIZE {
        return Err(anyhow!("buffer to big {} vs {MAX_SIZE}", data_buf.len()));
    }
    let len_buf = (data_buf.len() as u16).to_be_bytes();

    writer.write_all(&len_buf).await?;
    writer.write_all(data_buf).await?;
    Ok(())
}
