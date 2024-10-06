use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct TlsDestinationDetector {}

impl ProxyDestinationDetector for TlsDestinationDetector {
    async fn determine(&self, stream: &mut TcpStream) -> anyhow::Result<ProxyDestination> {
        todo!()
    }
}
