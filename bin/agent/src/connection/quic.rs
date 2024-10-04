use anyhow::anyhow;
use helper::configure_client;
use protocol::{key::AgentSigner, stream::TunnelStream};
use rustls::pki_types::CertificateDer;
use serde::de::DeserializeOwned;
use std::net::ToSocketAddrs;
use url::Url;

use quinn::{Endpoint, RecvStream, SendStream};

mod helper;

use super::Connection;

pub type QuicSubConnection = TunnelStream<RecvStream, SendStream>;

pub struct QuicConnection<RES> {
    response: RES,
    connection: quinn::Connection,
}

impl<RES: DeserializeOwned> QuicConnection<RES> {
    pub async fn new<AS: AgentSigner<RES>>(url: Url, agent_signer: &AS, server_certs: &[CertificateDer<'static>], allow_quic_insecure: bool) -> anyhow::Result<Self> {
        let url_host = url.host_str().ok_or(anyhow!("InvalidUrl"))?;
        let url_port = url.port().unwrap_or(33333);
        log::info!("connecting to server {}:{}", url_host, url_port);
        let remote = (url_host, url_port).to_socket_addrs()?.next().ok_or(anyhow!("DnsError"))?;

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().expect(""))?;
        endpoint.set_default_client_config(configure_client(server_certs, allow_quic_insecure)?);

        // connect to server
        let connection = endpoint.connect(remote, url_host)?.await?;

        log::info!("connected to {}, open bi stream", url);
        let (mut send_stream, mut recv_stream) = connection.open_bi().await?;
        log::info!("opened bi stream, send register request");

        send_stream.write_all(&agent_signer.sign_connect_req()).await?;

        let mut buf = [0u8; 4096];
        let buf_len = recv_stream.read(&mut buf).await?.ok_or(anyhow!("NoData"))?;
        let response: RES = agent_signer.validate_connect_res(&buf[..buf_len]).map_err(|e| anyhow!("validate connect rest error {e}"))?;
        Ok(Self { connection, response })
    }

    pub fn response(&self) -> &RES {
        &self.response
    }
}

#[async_trait::async_trait]
impl<RES: Send + Sync> Connection<QuicSubConnection> for QuicConnection<RES> {
    async fn create_outgoing(&mut self) -> anyhow::Result<QuicSubConnection> {
        let (send, recv) = self.connection.open_bi().await?;
        Ok(QuicSubConnection::new(recv, send))
    }

    async fn recv(&mut self) -> anyhow::Result<QuicSubConnection> {
        let (send, recv) = self.connection.accept_bi().await?;
        Ok(QuicSubConnection::new(recv, send))
    }
}
