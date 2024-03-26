use std::{error::Error, fmt::Debug, marker::PhantomData, net::SocketAddr, sync::Arc};

use protocol::key::ClusterValidator;
use quinn::{Endpoint, RecvStream, SendStream, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::de::DeserializeOwned;

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentQuicListener<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Debug> {
    endpoint: Endpoint,
    cluster_validator: VALIDATE,
    _tmp: PhantomData<REQ>,
}

impl<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Debug>
    AgentQuicListener<VALIDATE, REQ>
{
    pub async fn new(addr: SocketAddr, cluster_validator: VALIDATE) -> Self {
        log::info!("AgentQuicListener::new {}", addr);
        let (endpoint, _server_cert) =
            make_server_endpoint(addr).expect("Should make server endpoint");

        Self {
            endpoint,
            cluster_validator,
            _tmp: Default::default(),
        }
    }

    async fn process_incoming_conn(
        &self,
        conn: quinn::Connection,
    ) -> Result<AgentQuicConnection, Box<dyn Error>> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let mut buf = [0u8; 4096];
        let buf_len = recv
            .read(&mut buf)
            .await?
            .ok_or::<Box<dyn Error>>("No incomming data".into())?;

        match self.cluster_validator.validate_connect_req(&buf[..buf_len]) {
            Ok(request) => match self.cluster_validator.generate_domain(&request) {
                Ok(domain) => {
                    log::info!("register request domain {}", domain);
                    let res_buf = self.cluster_validator.sign_response_res(&request, None);
                    send.write_all(&res_buf).await?;
                    Ok(AgentQuicConnection { domain, conn })
                }
                Err(e) => {
                    log::error!("invalid register request {:?}, error {}", request, e);
                    let res_buf = self
                        .cluster_validator
                        .sign_response_res(&request, Some(e.clone()));
                    send.write_all(&res_buf).await?;
                    Err(e.into())
                }
            },
            Err(e) => {
                log::error!("register request error {:?}", e);
                Err(e.into())
            }
        }
    }
}

#[async_trait::async_trait]
impl<
        VALIDATE: ClusterValidator<REQ> + Send + Sync,
        REQ: DeserializeOwned + Send + Sync + Debug,
    > AgentListener<AgentQuicConnection, AgentQuicSubConnection, RecvStream, SendStream>
    for AgentQuicListener<VALIDATE, REQ>
{
    async fn recv(&mut self) -> Result<AgentQuicConnection, Box<dyn Error>> {
        loop {
            let incoming_conn = self
                .endpoint
                .accept()
                .await
                .ok_or::<Box<dyn Error>>("Cannot accept".into())?;
            let conn: quinn::Connection = incoming_conn.await?;
            log::info!(
                "[AgentQuicListener] new conn from {}",
                conn.remote_address()
            );
            match self.process_incoming_conn(conn).await {
                Ok(connection) => {
                    log::info!("new connection {}", connection.domain());
                    return Ok(connection);
                }
                Err(e) => {
                    log::error!("process_incoming_conn error: {}", e);
                }
            }
        }
    }
}

pub struct AgentQuicConnection {
    domain: String,
    conn: quinn::Connection,
}

#[async_trait::async_trait]
impl AgentConnection<AgentQuicSubConnection, RecvStream, SendStream> for AgentQuicConnection {
    fn domain(&self) -> String {
        self.domain.clone()
    }

    async fn create_sub_connection(&mut self) -> Result<AgentQuicSubConnection, Box<dyn Error>> {
        let (send, recv) = self.conn.open_bi().await?;
        Ok(AgentQuicSubConnection { send, recv })
    }

    async fn recv(&mut self) -> Result<AgentQuicSubConnection, Box<dyn Error>> {
        let (send, recv) = self.conn.accept_bi().await?;
        Ok(AgentQuicSubConnection { send, recv })
    }
}

pub struct AgentQuicSubConnection {
    send: SendStream,
    recv: RecvStream,
}

impl AgentSubConnection<RecvStream, SendStream> for AgentQuicSubConnection {
    fn split(self) -> (RecvStream, SendStream) {
        (self.recv, self.send)
    }
}

fn make_server_endpoint(bind_addr: SocketAddr) -> Result<(Endpoint, Vec<u8>), Box<dyn Error>> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> Result<(ServerConfig, Vec<u8>), Box<dyn Error>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    let priv_key = PrivatePkcs8KeyDer::from(priv_key);
    let cert_chain = vec![CertificateDer::from(cert_der.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key.into())?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}
