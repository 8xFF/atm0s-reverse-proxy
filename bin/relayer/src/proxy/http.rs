use tokio::io::{AsyncRead, AsyncWrite};

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct HttpDestinationDetector {}

impl ProxyDestinationDetector for HttpDestinationDetector {
    async fn determine<S: AsyncRead + AsyncWrite>(&self, stream: &mut S) -> anyhow::Result<ProxyDestination> {
        todo!()
    }
}
