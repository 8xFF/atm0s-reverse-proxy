use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite};
use protocol::stream::NamedStream;
use quinn::{ConnectionError, RecvStream, SendStream};

use crate::SubConnection;

pub struct QuicSubConnection {
    send: SendStream,
    recv: RecvStream,
}

impl QuicSubConnection {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        Self { send, recv }
    }
}

impl SubConnection for QuicSubConnection {}

impl NamedStream for QuicSubConnection {
    fn name(&self) -> &'static str {
        "out-proxy-quic-tunnel"
    }
}

impl AsyncRead for QuicSubConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.recv.poll_read(cx, buf) {
            // TODO: refactor
            // Quinn seems to have some issue with close connection. the Writer side already close but
            // Reader side only fire bellow error
            Poll::Ready(Err(quinn::ReadError::ConnectionLost(
                ConnectionError::ApplicationClosed(_),
            ))) => Poll::Ready(Ok(0)),
            e => e.map_err(|e| e.into()),
        }
    }
}

impl AsyncWrite for QuicSubConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.send)
            .poll_write(cx, buf)
            .map_err(|e| e.into())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.send).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.send).poll_close(cx)
    }
}
