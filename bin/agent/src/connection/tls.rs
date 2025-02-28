use std::net::ToSocketAddrs;

use anyhow::anyhow;
use futures::StreamExt;
use protocol::key::AgentSigner;
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_native_tls::{native_tls::Certificate, TlsConnector, TlsStream};
use tokio_yamux::Session;
use url::Url;

use crate::TcpSubConnection;

use super::Connection;

pub struct TlsConnection<RES> {
    response: RES,
    session: Session<TlsStream<TcpStream>>,
}

impl<RES: DeserializeOwned> TlsConnection<RES> {
    pub async fn new<AS: AgentSigner<RES>>(url: Url, agent_signer: &AS, rootca: Certificate, allow_unsecure: bool) -> anyhow::Result<Self> {
        let url_host = url.host_str().ok_or(anyhow!("couldn't get host from url"))?;
        let url_port = url.port().unwrap_or(33333);

        log::info!("connecting to server {}:{}", url_host, url_port);
        let remote = (url_host, url_port).to_socket_addrs()?.next().ok_or(anyhow!("couldn't resolve to an address"))?;
        let stream = TcpStream::connect(remote).await?;

        let connector_builder = tokio_native_tls::native_tls::TlsConnector::builder()
            .danger_accept_invalid_hostnames(true)
            .danger_accept_invalid_certs(allow_unsecure)
            .add_root_certificate(rootca)
            .build()?;
        let connector = TlsConnector::from(connector_builder);

        let mut tls_stream = connector.connect(url_host, stream).await?;

        tls_stream.write_all(&agent_signer.sign_connect_req()).await?;

        let mut buf = [0u8; 4096];
        let buf_len = tls_stream.read(&mut buf).await?;
        let response: RES = agent_signer.validate_connect_res(&buf[..buf_len]).map_err(|e| anyhow!("{e}"))?;
        Ok(Self {
            session: Session::new_server(tls_stream, Default::default()),
            response,
        })
    }

    pub fn response(&self) -> &RES {
        &self.response
    }
}

#[async_trait::async_trait]
impl<RES: Send + Sync> Connection<TcpSubConnection> for TlsConnection<RES> {
    async fn create_outgoing(&mut self) -> anyhow::Result<TcpSubConnection> {
        let stream = self.session.open_stream()?;
        Ok(stream)
    }

    async fn recv(&mut self) -> anyhow::Result<TcpSubConnection> {
        let stream = self.session.next().await.ok_or(anyhow!("accept new connection error"))??;
        Ok(stream)
    }
}
