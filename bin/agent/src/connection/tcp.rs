use std::net::ToSocketAddrs;

use super::{Connection, SubConnection};
use anyhow::anyhow;
use futures::prelude::*;
use protocol::key::AgentSigner;
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_yamux::{Session, StreamHandle};
use url::Url;

pub type TcpSubConnection = StreamHandle;

impl SubConnection for StreamHandle {}

pub struct TcpConnection<RES> {
    response: RES,
    session: Session<TcpStream>,
}

impl<RES: DeserializeOwned> TcpConnection<RES> {
    pub async fn new<AS: AgentSigner<RES>>(url: Url, agent_signer: &AS) -> anyhow::Result<Self> {
        let url_host = url.host_str().ok_or(anyhow!("couldn't get host from url"))?;
        let url_port = url.port().unwrap_or(33333);
        log::info!("connecting to server {}:{}", url_host, url_port);
        let remote = (url_host, url_port).to_socket_addrs()?.next().ok_or(anyhow!("couldn't resolve to an address"))?;

        let mut stream = TcpStream::connect(remote).await?;
        stream.write_all(&agent_signer.sign_connect_req()).await?;

        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await?;
        let response: RES = agent_signer.validate_connect_res(&buf[..buf_len]).map_err(|e| anyhow!("{e}"))?;
        Ok(Self {
            session: Session::new_server(stream, Default::default()),
            response,
        })
    }

    pub fn response(&self) -> &RES {
        &self.response
    }
}

#[async_trait::async_trait]
impl<RES: Send + Sync> Connection<TcpSubConnection> for TcpConnection<RES> {
    async fn create_outgoing(&mut self) -> anyhow::Result<TcpSubConnection> {
        let stream = self.session.open_stream()?;
        Ok(stream)
    }

    async fn recv(&mut self) -> anyhow::Result<TcpSubConnection> {
        let stream = self.session.next().await.ok_or(anyhow!("accept new connection error"))??;
        Ok(stream)
    }
}
