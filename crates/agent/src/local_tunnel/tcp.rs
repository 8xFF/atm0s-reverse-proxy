use std::{net::SocketAddr, error::Error};

use async_std::net::TcpStream;
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncReadExt,
};

use super::LocalTunnel;

pub struct LocalTcpTunnel {
    stream: TcpStream,
}

impl LocalTcpTunnel {
    pub async fn new(dest: SocketAddr) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            stream: TcpStream::connect(dest).await?,
        })
    }
}

impl LocalTunnel<ReadHalf<TcpStream>, WriteHalf<TcpStream>> for LocalTcpTunnel {
    fn split(self) -> (ReadHalf<TcpStream>, WriteHalf<TcpStream>) {
        AsyncReadExt::split(self.stream)
    }
}
