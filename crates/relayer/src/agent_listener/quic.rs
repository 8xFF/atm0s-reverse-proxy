use std::{
    error::Error,
    fmt::Debug,
    marker::PhantomData,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use async_std::channel::Receiver;
use futures::{AsyncRead, AsyncWrite};
use metrics::histogram;
use protocol::{key::ClusterValidator, stream::NamedStream};
use quinn::{ConnectionError, Endpoint, RecvStream, SendStream, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::de::DeserializeOwned;

use crate::METRICS_AGENT_HISTOGRAM;

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentQuicListener<REQ: DeserializeOwned + Debug> {
    rx: Receiver<AgentQuicConnection>,
    _tmp: PhantomData<REQ>,
}

impl<REQ: DeserializeOwned + Debug> AgentQuicListener<REQ> {
    pub async fn new<VALIDATE: 'static + ClusterValidator<REQ>>(
        addr: SocketAddr,
        cluster_validator: VALIDATE,
        priv_key: PrivatePkcs8KeyDer<'static>,
        cert: CertificateDer<'static>,
    ) -> Self {
        log::info!("AgentQuicListener::new {}", addr);
        let (tx, rx) = async_std::channel::bounded(100);
        async_std::task::spawn_local(async move {
            let endpoint =
                make_server_endpoint(addr, priv_key, cert).expect("Should make server endpoint");
            let cluster_validator = Arc::new(cluster_validator);
            while let Some(incoming_conn) = endpoint.accept().await {
                let cluster_validator = cluster_validator.clone();
                let tx = tx.clone();
                async_std::task::spawn_local(async move {
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
    ) -> Result<AgentQuicConnection, Box<dyn Error>> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let mut buf = [0u8; 4096];
        let buf_len = recv
            .read(&mut buf)
            .await?
            .ok_or::<Box<dyn Error>>("No incoming data".into())?;

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
                    let res_buf = cluster_validator.sign_response_res(&request, Some(e.clone()));
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

impl<REQ: DeserializeOwned + Send + Sync + Debug>
    AgentListener<AgentQuicConnection, AgentQuicSubConnection> for AgentQuicListener<REQ>
{
    async fn recv(&mut self) -> Result<AgentQuicConnection, Box<dyn Error>> {
        self.rx.recv().await.map_err(|e| e.into())
    }
}

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

impl AgentSubConnection for AgentQuicSubConnection {}

impl NamedStream for AgentQuicSubConnection {
    fn name(&self) -> &'static str {
        "agent-quic"
    }
}

impl AsyncRead for AgentQuicSubConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        match this.recv.poll_read(cx, buf) {
            // TODO refactor
            // Quinn seems to have some issue with close connection. the Writer side already close but
            // Reader side only fire bellow error
            Poll::Ready(Err(quinn::ReadError::ConnectionLost(
                ConnectionError::ConnectionClosed(_),
            ))) => Poll::Ready(Ok(0)),
            e => e.map_err(|e| e.into()),
        }
    }
}

impl AsyncWrite for AgentQuicSubConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.send)
            .poll_write(cx, buf)
            .map_err(|e| e.into())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.send).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.send).poll_close(cx)
    }
}

fn make_server_endpoint(
    bind_addr: SocketAddr,
    priv_key: PrivatePkcs8KeyDer<'static>,
    cert: CertificateDer<'static>,
) -> Result<Endpoint, Box<dyn Error>> {
    let server_config = configure_server(priv_key, cert)?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok(endpoint)
}

/// Returns default server configuration along with its certificate.
fn configure_server(
    priv_key: PrivatePkcs8KeyDer<'static>,
    cert: CertificateDer<'static>,
) -> Result<ServerConfig, Box<dyn Error>> {
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
