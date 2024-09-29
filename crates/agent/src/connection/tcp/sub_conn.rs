use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite};
use protocol::stream::NamedStream;

use crate::SubConnection;

pub struct TcpSubConnection {
    stream: yamux::Stream,
}

impl From<yamux::Stream> for TcpSubConnection {
    fn from(stream: yamux::Stream) -> Self {
        Self { stream }
    }
}

impl SubConnection for TcpSubConnection {}

impl NamedStream for TcpSubConnection {
    fn name(&self) -> &'static str {
        "yamux-tcp"
    }
}

impl AsyncRead for TcpSubConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for TcpSubConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }
}
