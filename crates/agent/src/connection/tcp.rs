use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use async_std::net::TcpStream;
use futures::io::{ReadHalf, WriteHalf};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Future};
use protocol::{key::LocalKey, rpc::RegisterResponse};
use yamux::Mode;

use super::{Connection, SubConnection};

pub struct TcpSubConnection {
    stream: yamux::Stream,
}

impl TcpSubConnection {
    pub fn new(stream: yamux::Stream) -> Self {
        Self { stream }
    }
}

impl SubConnection<ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>> for TcpSubConnection {
    fn split(self) -> (ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>) {
        AsyncReadExt::split(self.stream)
    }
}

pub struct TcpConnection {
    conn: yamux::Connection<TcpStream>,
    #[allow(unused)]
    domain: String,
}

impl TcpConnection {
    pub async fn new(dest: SocketAddr, local_key: &LocalKey) -> Option<Self> {
        let mut stream = TcpStream::connect(dest).await.ok()?;
        let request = local_key.to_request();
        let request_buf: Vec<u8> = (&request).into();
        stream.write_all(&request_buf).await.ok()?;

        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await.ok()?;
        let response = RegisterResponse::try_from(&buf[..buf_len]).ok()?;
        match response.response {
            Ok(domain) => {
                log::info!("registed domain {}", domain);
                Some(Self {
                    conn: yamux::Connection::new(stream, Default::default(), Mode::Server),
                    domain,
                })
            }
            Err(e) => {
                log::error!("register response error {}", e);
                return None;
            }
        }
    }
}

#[async_trait::async_trait]
impl Connection<TcpSubConnection, ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>>
    for TcpConnection
{
    async fn recv(&mut self) -> Option<TcpSubConnection> {
        let mux_server = YamuxConnectionServer::new(&mut self.conn);
        match mux_server.await {
            Ok(Some(stream)) => Some(TcpSubConnection::new(stream)),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct YamuxConnectionServer<'a, T> {
    connection: &'a mut yamux::Connection<T>,
}

impl<'a, T> YamuxConnectionServer<'a, T> {
    pub fn new(connection: &'a mut yamux::Connection<T>) -> Self {
        Self { connection }
    }
}

impl<'a, T> Future for YamuxConnectionServer<'a, T>
where
    T: AsyncRead + AsyncWrite + Unpin + std::fmt::Debug,
{
    type Output = yamux::Result<Option<yamux::Stream>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.connection.poll_next_inbound(cx)? {
            Poll::Ready(stream) => {
                log::info!("YamuxConnectionServer new stream");
                Poll::Ready(Ok(stream))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
