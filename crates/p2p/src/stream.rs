use std::{fmt::Debug, marker::PhantomData};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use serde::{de::DeserializeOwned, Serialize};
use tokio_util::{
    bytes::{Buf, BufMut},
    codec::{Decoder, Encoder},
};

use quinn::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct QuicStream {
    read: RecvStream,
    write: SendStream,
}

impl QuicStream {
    pub fn new(read: RecvStream, write: SendStream) -> Self {
        Self { read, write }
    }
}

impl AsyncRead for QuicStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().read).poll_read(cx, buf)
    }
}

impl AsyncWrite for QuicStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        let w: &mut (dyn AsyncWrite + Unpin) = &mut self.get_mut().write;
        Pin::new(w).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_shutdown(cx)
    }
}

pub struct BincodeCodec<Item> {
    _tmp: PhantomData<Item>,
}

impl<Item> Default for BincodeCodec<Item> {
    fn default() -> Self {
        Self { _tmp: Default::default() }
    }
}

impl<Item: Serialize> Encoder<Item> for BincodeCodec<Item> {
    type Error = bincode::Error;

    fn encode(&mut self, item: Item, dst: &mut tokio_util::bytes::BytesMut) -> Result<(), Self::Error> {
        let res = bincode::serialize_into(dst.writer(), &item);
        res
    }
}

impl<Item: DeserializeOwned + Debug> Decoder for BincodeCodec<Item> {
    type Error = bincode::Error;
    type Item = Item;

    fn decode(&mut self, src: &mut tokio_util::bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Result::<_, Self::Error>::Ok(Option::<Self::Item>::None);
        }
        let res = bincode::deserialize_from::<_, Item>(src.reader()).map(|o| Some(o));
        res
    }
}
