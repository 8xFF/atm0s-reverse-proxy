use std::{
    error::Error,
    io,
    net::{Shutdown, SocketAddr},
    pin::Pin,
    task::{Context, Poll},
};

use super::LocalTunnel;
use async_std::net::TcpStream;
use futures::{AsyncRead, AsyncWrite};
use protocol::stream::NamedStream;

pub struct LocalTcpTunnel {
    stream: TcpStream,
}

impl LocalTcpTunnel {
    pub async fn connect(dest: SocketAddr) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            stream: TcpStream::connect(dest).await?,
        })
    }
}

impl From<TcpStream> for LocalTcpTunnel {
    fn from(stream: TcpStream) -> Self {
        Self { stream }
    }
}

impl LocalTunnel for LocalTcpTunnel {}

impl NamedStream for LocalTcpTunnel {
    fn name(&self) -> &'static str {
        "local-tcp-tunnel"
    }
}

impl AsyncRead for LocalTcpTunnel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for LocalTcpTunnel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.stream).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        // We need to call shutdown here, without it the proxy will stuck forever
        Poll::Ready(Pin::new(&mut this.stream).shutdown(Shutdown::Write))
    }
}
