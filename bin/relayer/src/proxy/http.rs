use anyhow::anyhow;
use tokio::net::TcpStream;

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct HttpDestinationDetector {}

impl ProxyDestinationDetector for HttpDestinationDetector {
    async fn determine(&self, stream: &mut TcpStream) -> anyhow::Result<ProxyDestination> {
        let mut buf = [0; 4096];
        let buf_len = stream.peek(&mut buf).await?;
        log::info!("[HttpDomainDetector] check domain for {}", String::from_utf8_lossy(&buf[..buf_len]));
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        let _ = req.parse(&buf[..buf_len])?;
        let domain = req.headers.iter().find(|h| h.name.to_lowercase() == "host").ok_or(anyhow!("host header missing"))?.value;
        // dont get the port
        let domain = String::from_utf8_lossy(domain);
        let domain = domain.split(':').next().expect("should have domain");
        Ok(ProxyDestination {
            domain: domain.to_string(),
            service: None,
            tls: false,
        })
    }
}
