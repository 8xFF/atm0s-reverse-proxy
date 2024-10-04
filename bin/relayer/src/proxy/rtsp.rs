use tokio::io::{AsyncRead, AsyncWrite};

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct RtspDestinationDetector {}

impl ProxyDestinationDetector for RtspDestinationDetector {
    async fn determine<S: AsyncRead + AsyncWrite>(
        &self,
        stream: &mut S,
    ) -> anyhow::Result<ProxyDestination> {
        todo!()
    }
}
