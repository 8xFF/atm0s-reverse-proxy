use crate::proxy_listener::DomainDetector;

#[derive(Default)]
pub struct RtspDomainDetector();

impl DomainDetector for RtspDomainDetector {
    fn get_domain(&self, buf: &[u8]) -> Option<String> {
        let (message, _consumed): (rtsp_types::Message<Vec<u8>>, _) =
            rtsp_types::Message::parse(buf).ok()?;
        log::info!("{:?}", message);
        match message {
            rtsp_types::Message::Request(req) => req.request_uri()?.host().map(|h| h.to_string()),
            _ => None,
        }
    }
}
