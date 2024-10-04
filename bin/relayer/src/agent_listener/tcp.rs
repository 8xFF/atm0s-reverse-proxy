use std::{fmt::Debug, net::SocketAddr, time::Instant};

use anyhow::anyhow;
use futures::StreamExt;
use metrics::histogram;
use protocol::key::ClusterValidator;
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_yamux::{Session, StreamHandle};

use crate::METRICS_AGENT_HISTOGRAM;

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentTcpListener<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Debug> {
    tcp_listener: TcpListener,
    cluster_validator: VALIDATE,
    _tmp: std::marker::PhantomData<REQ>,
}

impl<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Debug>
    AgentTcpListener<VALIDATE, REQ>
{
    pub async fn new(addr: SocketAddr, cluster_validator: VALIDATE) -> Self {
        log::info!("AgentTcpListener::new {}", addr);
        Self {
            tcp_listener: TcpListener::bind(addr).await.expect("Should open"),
            cluster_validator,
            _tmp: Default::default(),
        }
    }

    async fn process_incoming_stream(
        &self,
        mut stream: TcpStream,
    ) -> anyhow::Result<AgentTcpConnection> {
        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await?;

        let res = self.cluster_validator.validate_connect_req(&buf[..buf_len]);

        match res {
            Ok(request) => match self.cluster_validator.generate_domain(&request) {
                Ok(domain) => {
                    log::info!("register request domain {}", domain);
                    let res = self.cluster_validator.sign_response_res(&request, None);
                    stream.write_all(&res).await?;
                    Ok(AgentTcpConnection {
                        domain,
                        conn_id: rand::random(),
                        session: Session::new_client(stream, Default::default()),
                    })
                }
                Err(e) => {
                    log::error!("invalid register request {:?}, err {}", request, e);
                    let res = self
                        .cluster_validator
                        .sign_response_res(&request, Some(e.to_string()));
                    stream.write_all(&res).await?;
                    Err(anyhow!("generate domain error: {e}"))
                }
            },
            Err(e) => {
                log::error!("register request error {:?}", e);
                Err(anyhow!("validate connect req error: {e}"))
            }
        }
    }
}

impl<
        VALIDATE: ClusterValidator<REQ> + Send + Sync,
        REQ: DeserializeOwned + Send + Sync + Debug,
    > AgentListener<AgentTcpConnection, AgentTcpSubConnection> for AgentTcpListener<VALIDATE, REQ>
{
    async fn recv(&mut self) -> anyhow::Result<AgentTcpConnection> {
        loop {
            let (stream, remote) = self.tcp_listener.accept().await?;
            log::info!("[AgentTcpListener] new conn from {}", remote);
            let started = Instant::now();
            match self.process_incoming_stream(stream).await {
                Ok(connection) => {
                    histogram!(METRICS_AGENT_HISTOGRAM)
                        .record(started.elapsed().as_millis() as f32 / 1000.0);
                    log::info!("new connection {}", connection.domain());
                    return Ok(connection);
                }
                Err(e) => {
                    log::error!("process_incoming_stream error: {}", e);
                }
            }
        }
    }
}

pub type AgentTcpSubConnection = StreamHandle;
impl AgentSubConnection for AgentTcpSubConnection {}

pub struct AgentTcpConnection {
    domain: String,
    conn_id: u64,
    session: Session<TcpStream>,
}

impl AgentConnection<AgentTcpSubConnection> for AgentTcpConnection {
    fn domain(&self) -> String {
        self.domain.clone()
    }

    fn conn_id(&self) -> u64 {
        self.conn_id
    }

    async fn create_sub_connection(&mut self) -> anyhow::Result<AgentTcpSubConnection> {
        let stream = self.session.open_stream()?;
        Ok(stream)
    }

    async fn recv(&mut self) -> anyhow::Result<AgentTcpSubConnection> {
        let stream = self
            .session
            .next()
            .await
            .ok_or(anyhow!("accept new connection error"))??;
        Ok(stream)
    }
}
