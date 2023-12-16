use std::{net::{SocketAddr, Ipv4Addr}, pin::Pin, task::{Context, Poll}};

use async_std::net::{TcpListener, TcpStream};
use futures::{io::{ReadHalf, WriteHalf}, AsyncReadExt, Future, AsyncRead, AsyncWrite};

use super::{AgentListener, AgentConnection, AgentSubConnection};

pub struct AgentTcpListener {
    tcp_listener: TcpListener,
}

impl AgentTcpListener {
    pub async fn new(port: u16) -> Option<Self> {
        log::info!("AgentTcpListener::new {}", port);
        Some(Self {
            tcp_listener: TcpListener::bind(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port)).await.ok()?,
        })
    }
}

#[async_trait::async_trait]
impl AgentListener<AgentTcpConnection, AgentTcpSubConnection, ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>> for AgentTcpListener {
    async fn recv(&mut self) -> Option<AgentTcpConnection> {
        let (mut stream, remote) = self.tcp_listener.accept().await.ok()?;
        log::info!("[AgentTcpListener] new conn from {}", remote);

        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await.ok()?;

        Some(AgentTcpConnection {
            domain: String::from_utf8_lossy(&buf[..buf_len]).to_string(),
            connector: yamux::Connection::new(stream, Default::default(), yamux::Mode::Client),
        })
    }
}

pub struct AgentTcpConnection {
    domain: String,
    connector: yamux::Connection<TcpStream>,
}

#[async_trait::async_trait]
impl AgentConnection<AgentTcpSubConnection, ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>> for AgentTcpConnection {
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
        }.await
    }
}

pub struct AgentTcpSubConnection {
    stream: yamux::Stream
}

impl AgentSubConnection<ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>> for AgentTcpSubConnection {
    fn split (self) -> (ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>) {
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
            Poll::Ready(stream) => {
                return Poll::Ready(stream)
            }
            Poll::Pending => Poll::Pending
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
            Poll::Ready(Some(Ok(_stream))) => {
                return Poll::Ready(Some(()))
            }
            Poll::Ready(Some(Err(_e))) => {
                return Poll::Ready(None)
            }
            Poll::Ready(None) => {
                return Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending
        }
    }
}