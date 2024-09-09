use crate::proxy_listener::DomainDetector;

#[derive(Default)]
pub struct HttpDomainDetector();

impl DomainDetector for HttpDomainDetector {
    fn name(&self) -> &str {
        "http"
    }

    fn get_domain(&self, buf: &[u8]) -> Option<String> {
        log::info!(
            "[HttpDomainDetector] check domain for {}",
            String::from_utf8_lossy(buf)
        );
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        let _ = req.parse(buf).ok()?;
        let domain = req
            .headers
            .iter()
            .find(|h| h.name.to_lowercase() == "host")?
            .value;
        // dont get the port
        let domain = String::from_utf8_lossy(domain).to_string();
        let domain = domain.split(':').next()?;
        Some(domain.to_string())
    }
}
