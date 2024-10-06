use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct RtspDestinationDetector {}

impl ProxyDestinationDetector for RtspDestinationDetector {
    async fn determine(&self, stream: &mut TcpStream) -> anyhow::Result<ProxyDestination> {
        todo!()
    }
}
