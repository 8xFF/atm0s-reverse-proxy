use std::{
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
    task::{Context, Poll},
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
    pub async fn new(port: u16, root_domain: String) -> Option<Self> {
        log::info!("AgentTcpListener::new {}", port);
        Some(Self {
            tcp_listener: TcpListener::bind(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port))
                .await
                .ok()?,
            root_domain,
        })
    }

    async fn process_incoming_stream(&self, mut stream: TcpStream) -> Option<AgentTcpConnection> {
        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await.ok()?;

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
                    return None;
                }

                let domain = res.response.ok()?;
                Some(AgentTcpConnection {
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
                None
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
    async fn recv(&mut self) -> Option<AgentTcpConnection> {
        loop {
            let (stream, remote) = self.tcp_listener.accept().await.ok()?;
            log::info!("[AgentTcpListener] new conn from {}", remote);
            if let Some(connection) = self.process_incoming_stream(stream).await {
                return Some(connection);
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

    async fn create_sub_connection(&mut self) -> Option<AgentTcpSubConnection> {
        let client = OpenStreamsClient {
            connection: &mut self.connector,
        };
        Some(AgentTcpSubConnection {
            stream: client.await.ok()?,
        })
    }

    async fn recv(&mut self) -> Option<()> {
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
    type Output = Option<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.connection.poll_next_inbound(cx) {
            Poll::Ready(Some(Ok(_stream))) => return Poll::Ready(Some(())),
            Poll::Ready(Some(Err(_e))) => return Poll::Ready(None),
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
