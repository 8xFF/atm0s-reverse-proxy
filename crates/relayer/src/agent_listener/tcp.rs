use std::{
    error::Error,
    fmt::Debug,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use async_std::net::{TcpListener, TcpStream};
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Future,
};
use protocol::key::ClusterValidator;
use serde::de::DeserializeOwned;

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
    ) -> Result<AgentTcpConnection, Box<dyn Error>> {
        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await?;

        match self.cluster_validator.validate_connect_req(&buf[..buf_len]) {
            Ok(request) => match self.cluster_validator.generate_domain(&request) {
                Ok(domain) => {
                    log::info!("register request domain {}", domain);
                    let res = self.cluster_validator.sign_response_res(&request, None);
                    stream.write_all(&res).await?;
                    Ok(AgentTcpConnection {
                        domain,
                        connector: yamux::Connection::new(
                            stream,
                            Default::default(),
                            yamux::Mode::Client,
                        ),
                    })
                }
                Err(e) => {
                    log::error!("invalid register request {:?}, err {}", request, e);
                    let res = self
                        .cluster_validator
                        .sign_response_res(&request, Some(e.clone()));
                    stream.write_all(&res).await?;
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
    >
    AgentListener<
        AgentTcpConnection,
        AgentTcpSubConnection,
        ReadHalf<yamux::Stream>,
        WriteHalf<yamux::Stream>,
    > for AgentTcpListener<VALIDATE, REQ>
{
    async fn recv(&mut self) -> Result<AgentTcpConnection, Box<dyn Error>> {
        loop {
            let (stream, remote) = self.tcp_listener.accept().await?;
            log::info!("[AgentTcpListener] new conn from {}", remote);
            match self.process_incoming_stream(stream).await {
                Ok(connection) => {
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

pub struct AgentTcpConnection {
    domain: String,
    connector: yamux::Connection<TcpStream>,
}

#[async_trait::async_trait]
impl AgentConnection<AgentTcpSubConnection, ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>>
    for AgentTcpConnection
{
    fn domain(&self) -> String {
        self.domain.clone()
    }

    async fn create_sub_connection(&mut self) -> Result<AgentTcpSubConnection, Box<dyn Error>> {
        let client = OpenStreamsClient {
            connection: &mut self.connector,
        };
        Ok(AgentTcpSubConnection {
            stream: client.await?,
        })
    }

    async fn recv(&mut self) -> Result<(), Box<dyn Error>> {
        RecvStreamsClient {
            connection: &mut self.connector,
        }
        .await
    }
}

pub struct AgentTcpSubConnection {
    stream: yamux::Stream,
}

impl AgentSubConnection<ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>>
    for AgentTcpSubConnection
{
    fn split(self) -> (ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>) {
        AsyncReadExt::split(self.stream)
    }
}

#[derive(Debug)]
pub struct OpenStreamsClient<'a, T> {
    connection: &'a mut yamux::Connection<T>,
}

impl<'a, T> Future for OpenStreamsClient<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin + std::fmt::Debug,
{
    type Output = yamux::Result<yamux::Stream>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.connection.poll_new_outbound(cx) {
            Poll::Ready(stream) => return Poll::Ready(stream),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug)]
pub struct RecvStreamsClient<'a, T> {
    connection: &'a mut yamux::Connection<T>,
}

impl<'a, T> Future for RecvStreamsClient<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin + std::fmt::Debug,
{
    type Output = Result<(), Box<dyn Error>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.connection.poll_next_inbound(cx) {
            Poll::Ready(Some(Ok(_stream))) => return Poll::Ready(Ok(())),
            Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e.into())),
            Poll::Ready(None) => {
                return Poll::Ready(Err("yamux server poll next inbound return None".into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
