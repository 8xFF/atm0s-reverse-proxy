use anyhow::anyhow;
use protocol::services::SERVICE_RTSP;
use tokio::net::TcpStream;

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct RtspDestinationDetector {}

impl ProxyDestinationDetector for RtspDestinationDetector {
    async fn determine(&self, stream: &mut TcpStream) -> anyhow::Result<ProxyDestination> {
        let mut buf = [0; 4096];
        let buf_len = stream.peek(&mut buf).await?;
        log::info!("[RtspDomainDetector] check domain for {}", String::from_utf8_lossy(&buf[..buf_len]));
        let (message, _consumed): (rtsp_types::Message<Vec<u8>>, _) = rtsp_types::Message::parse(&buf[..buf_len])?;
        log::info!("{:?}", message);
        match message {
            rtsp_types::Message::Request(req) => Ok(ProxyDestination {
                domain: req
                    .request_uri()
                    .ok_or(anyhow!("missing request uri"))?
                    .host()
                    .map(|h| h.to_string())
                    .ok_or(anyhow!("missing host header"))?,
                service: Some(SERVICE_RTSP),
                tls: false,
            }),
            _ => Err(anyhow!("invalid rtsp message")),
        }
    }
}
