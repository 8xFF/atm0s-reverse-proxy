use futures::{AsyncRead, AsyncWrite};

pub mod tcp;

pub trait LocalTunnel<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>: Send + Sync {
    fn split(self) -> (R, W);
}
