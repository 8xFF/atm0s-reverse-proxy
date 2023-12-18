use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll}, error::Error,
};

use async_std::net::{TcpListener, TcpStream};
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Future,
};
use protocol::{
    key::validate_request,
    rpc::{RegisterRequest, RegisterResponse},
};

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentTcpListener {
    tcp_listener: TcpListener,
    root_domain: String,
}

impl AgentTcpListener {
    pub async fn new(addr: SocketAddr, root_domain: String) -> Self {
        log::info!("AgentTcpListener::new {}", addr);
        Self {
            tcp_listener: TcpListener::bind(addr)
                .await.expect("Should open"),
            root_domain,
        }
    }

    async fn process_incoming_stream(&self, mut stream: TcpStream) -> Result<AgentTcpConnection, Box<dyn Error>> {
        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await?;

        match RegisterRequest::try_from(&buf[..buf_len]) {
            Ok(request) => {
                let response = if let Some(sub_domain) = validate_request(&request) {
                    log::info!("register request domain {}", sub_domain);
                    Ok(format!("{}.{}", sub_domain, self.root_domain))
                } else {
                    log::error!("invalid register request {:?}", request);
                    Err(String::from("invalid request"))
                };

                let res = RegisterResponse { response };
                let res_buf: Vec<u8> = (&res).into();
                if let Err(e) = stream.write_all(&res_buf).await {
                    log::error!("register response error {:?}", e);
                    return Err(e.into());
                }

                let domain = res.response?;
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
                log::error!("register request error {:?}", e);
                Err(e.into())
            }
        }
    }
}

#[async_trait::async_trait]
impl
    AgentListener<
        AgentTcpConnection,
        AgentTcpSubConnection,
        ReadHalf<yamux::Stream>,
        WriteHalf<yamux::Stream>,
    > for AgentTcpListener
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
            Poll::Ready(Some(Ok(stream))) => return Poll::Ready(Ok(())),
            Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e.into())),
            Poll::Ready(None) => return Poll::Ready(Err("yamux server poll next inbound return None".into())),
            Poll::Pending => Poll::Pending,
        }
    }
}
