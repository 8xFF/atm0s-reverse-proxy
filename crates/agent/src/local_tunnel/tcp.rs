use std::net::SocketAddr;

use async_std::net::TcpStream;
use futures::{AsyncReadExt, io::{ReadHalf, WriteHalf}};

use super::LocalTunnel;

pub struct LocalTcpTunnel {
    stream: TcpStream
}

impl LocalTcpTunnel {
    pub async fn new(dest: SocketAddr) -> Option<Self> {
        Some(Self {
            stream: TcpStream::connect(dest).await.ok()?,
        })
    }
}

impl LocalTunnel<ReadHalf<TcpStream>, WriteHalf<TcpStream>> for LocalTcpTunnel {
    fn split (self) -> (ReadHalf<TcpStream>, WriteHalf<TcpStream>) {
        AsyncReadExt::split(self.stream)
    }
    
}