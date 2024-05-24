use std::{
    error::Error,
    net::{SocketAddr, ToSocketAddrs},
    pin::Pin,
    task::{Context, Poll},
};

use async_std::net::TcpStream;
use futures::io::{ReadHalf, WriteHalf};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Future};
use protocol::key::AgentSigner;
use serde::de::DeserializeOwned;
use url::Url;
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

pub struct TcpConnection<RES> {
    response: RES,
    conn: yamux::Connection<TcpStream>,
}

impl<RES: DeserializeOwned> TcpConnection<RES> {
    pub async fn new<AS: AgentSigner<RES>>(
        url: Url,
        agent_signer: &AS,
    ) -> Result<Self, Box<dyn Error>> {
        let url_host = url
            .host_str()
            .ok_or::<Box<dyn Error>>("couldn't get host from url".into())?;
        let url_port = url.port().unwrap_or(33333);
        log::info!("connecting to server {}:{}", url_host, url_port);
        let remote = (url_host, url_port)
            .to_socket_addrs()?
            .next()
            .ok_or::<Box<dyn Error>>("couldn't resolve to an address".into())?;

        let mut stream = TcpStream::connect(remote).await?;
        stream.write_all(&agent_signer.sign_connect_req()).await?;

        let mut buf = [0u8; 4096];
        let buf_len = stream.read(&mut buf).await?;
        let response: RES = agent_signer.validate_connect_res(&buf[..buf_len])?;
        Ok(Self {
            conn: yamux::Connection::new(stream, Default::default(), Mode::Server),
            response,
        })
    }

    pub fn response(&self) -> &RES {
        &self.response
    }
}

#[async_trait::async_trait]
impl<RES: Send + Sync>
    Connection<TcpSubConnection, ReadHalf<yamux::Stream>, WriteHalf<yamux::Stream>>
    for TcpConnection<RES>
{
    async fn create_outgoing(&mut self) -> Result<TcpSubConnection, Box<dyn Error>> {
        todo!()
    }

    async fn recv(&mut self) -> Result<TcpSubConnection, Box<dyn Error>> {
        let mux_server = YamuxConnectionServer::new(&mut self.conn);
        match mux_server.await {
            Ok(Some(stream)) => Ok(TcpSubConnection::new(stream)),
            Ok(None) => Err("yamux server poll next inbound return None".into()),
            Err(e) => Err(e.into()),
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
