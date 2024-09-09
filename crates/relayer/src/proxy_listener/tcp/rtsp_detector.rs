use crate::proxy_listener::DomainDetector;

#[derive(Default)]
pub struct RtspDomainDetector();

impl DomainDetector for RtspDomainDetector {
    fn name(&self) -> &str {
        "rtsp"
    }

    fn get_domain(&self, buf: &[u8]) -> Option<String> {
        log::info!(
            "[RtspDomainDetector] check domain for {}",
            String::from_utf8_lossy(buf)
        );
        let (message, _consumed): (rtsp_types::Message<Vec<u8>>, _) =
            rtsp_types::Message::parse(buf).ok()?;
        log::info!("{:?}", message);
        match message {
            rtsp_types::Message::Request(req) => req.request_uri()?.host().map(|h| h.to_string()),
            _ => None,
        }
    }
}
