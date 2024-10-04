use std::{
    fmt::Debug,
    marker::PhantomData,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use metrics::histogram;
use protocol::{key::ClusterValidator, stream::TunnelStream};
use quinn::{Endpoint, RecvStream, SendStream, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::{channel, Receiver};

use crate::METRICS_AGENT_HISTOGRAM;

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentQuicListener<REQ: DeserializeOwned + Debug> {
    rx: Receiver<AgentQuicConnection>,
    _tmp: PhantomData<REQ>,
}

impl<REQ: DeserializeOwned + Send + Sync + Debug> AgentQuicListener<REQ> {
    pub async fn new<VALIDATE: Send + Sync + 'static + ClusterValidator<REQ>>(
        addr: SocketAddr,
        cluster_validator: VALIDATE,
        priv_key: PrivatePkcs8KeyDer<'static>,
        cert: CertificateDer<'static>,
    ) -> Self {
        log::info!("AgentQuicListener::new {}", addr);
        let (tx, rx) = channel(100);
        tokio::spawn(async move {
            let endpoint =
                make_server_endpoint(addr, priv_key, cert).expect("Should make server endpoint");
            let cluster_validator = Arc::new(cluster_validator);
            while let Some(incoming_conn) = endpoint.accept().await {
                let cluster_validator = cluster_validator.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    log::info!(
                        "[AgentQuicListener] On incoming from {}",
                        incoming_conn.remote_address()
                    );
                    let conn: quinn::Connection = match incoming_conn.await {
                        Ok(conn) => conn,
                        Err(e) => {
                            log::error!("[AgentQuicListener] incoming conn error {}", e);
                            return;
                        }
                    };
                    log::info!(
                        "[AgentQuicListener] new conn from {}",
                        conn.remote_address()
                    );
                    let started = Instant::now();
                    match Self::process_incoming_conn(cluster_validator, conn).await {
                        Ok(connection) => {
                            histogram!(METRICS_AGENT_HISTOGRAM)
                                .record(started.elapsed().as_millis() as f32 / 1000.0);
                            log::info!("new connection {}", connection.domain());
                            if let Err(e) = tx.send(connection).await {
                                log::error!("send new connection to main loop error {:?}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("process_incoming_conn error: {}", e);
                        }
                    }
                });
            }
        });

        Self {
            rx,
            _tmp: Default::default(),
        }
    }

    async fn process_incoming_conn<VALIDATE: ClusterValidator<REQ>>(
        cluster_validator: Arc<VALIDATE>,
        conn: quinn::Connection,
    ) -> anyhow::Result<AgentQuicConnection> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let mut buf = [0u8; 4096];
        let buf_len = recv
            .read(&mut buf)
            .await?
            .ok_or(anyhow!("no incoming data"))?;

        match cluster_validator.validate_connect_req(&buf[..buf_len]) {
            Ok(request) => match cluster_validator.generate_domain(&request) {
                Ok(domain) => {
                    log::info!("register request domain {}", domain);
                    let res_buf = cluster_validator.sign_response_res(&request, None);
                    send.write_all(&res_buf).await?;
                    Ok(AgentQuicConnection {
                        domain,
                        conn,
                        conn_id: rand::random(),
                    })
                }
                Err(e) => {
                    log::error!("invalid register request {:?}, error {}", request, e);
                    let res_buf =
                        cluster_validator.sign_response_res(&request, Some(e.to_string()));
                    send.write_all(&res_buf).await?;
                    Err(anyhow!("{e}"))
                }
            },
            Err(e) => {
                log::error!("register request error {:?}", e);
                Err(anyhow!("{e}"))
            }
        }
    }
}

impl<REQ: DeserializeOwned + Send + Sync + Debug>
    AgentListener<AgentQuicConnection, AgentQuicSubConnection> for AgentQuicListener<REQ>
{
    async fn recv(&mut self) -> anyhow::Result<AgentQuicConnection> {
        self.rx.recv().await.ok_or(anyhow!("InternalQueueError"))
    }
}

pub type AgentQuicSubConnection = TunnelStream<RecvStream, SendStream>;

impl AgentSubConnection for AgentQuicSubConnection {}

pub struct AgentQuicConnection {
    domain: String,
    conn_id: u64,
    conn: quinn::Connection,
}

impl AgentConnection<AgentQuicSubConnection> for AgentQuicConnection {
    fn domain(&self) -> String {
        self.domain.clone()
    }

    fn conn_id(&self) -> u64 {
        self.conn_id
    }

    async fn create_sub_connection(&mut self) -> anyhow::Result<AgentQuicSubConnection> {
        let (send, recv) = self.conn.open_bi().await?;
        Ok(AgentQuicSubConnection::new(recv, send))
    }

    async fn recv(&mut self) -> anyhow::Result<AgentQuicSubConnection> {
        let (send, recv) = self.conn.accept_bi().await?;
        Ok(AgentQuicSubConnection::new(recv, send))
    }
}

fn make_server_endpoint(
    bind_addr: SocketAddr,
    priv_key: PrivatePkcs8KeyDer<'static>,
    cert: CertificateDer<'static>,
) -> anyhow::Result<Endpoint> {
    let server_config = configure_server(priv_key, cert)?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok(endpoint)
}

/// Returns default server configuration along with its certificate.
fn configure_server(
    priv_key: PrivatePkcs8KeyDer<'static>,
    cert: CertificateDer<'static>,
) -> anyhow::Result<ServerConfig> {
    let cert_chain = vec![cert];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key.into())?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    transport_config.max_idle_timeout(Some(
        Duration::from_secs(30)
            .try_into()
            .expect("Should config timeout"),
    ));

    Ok(server_config)
}
