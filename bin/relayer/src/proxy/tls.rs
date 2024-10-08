use anyhow::anyhow;
use tls_parser::{parse_tls_extensions, parse_tls_plaintext, TlsExtension, TlsMessage, TlsMessageHandshake};
use tokio::net::TcpStream;

use super::{ProxyDestination, ProxyDestinationDetector};

#[derive(Debug, Default)]
pub struct TlsDestinationDetector {
    service: Option<u16>,
}

impl TlsDestinationDetector {
    pub fn custom_service(service: u16) -> Self {
        Self { service: Some(service) }
    }

    fn get_domain(&self, packet: &[u8]) -> Option<String> {
        log::info!("[TlsDomainDetector] check domain for buffer {} bytes", packet.len());
        let res = match parse_tls_plaintext(packet) {
            Ok(res) => res,
            Err(e) => {
                log::error!("parse_tls_plaintext error {:?}", e);
                return None;
            }
        };

        let tls_message = &res.1.msg[0];
        if let TlsMessage::Handshake(TlsMessageHandshake::ClientHello(client_hello)) = tls_message {
            // get the extensions
            let extensions: &[u8] = client_hello.ext?;
            // parse the extensions
            let res = match parse_tls_extensions(extensions) {
                Ok(res) => res,
                Err(e) => {
                    log::error!("parse_tls_extensions error {:?}", e);
                    return None;
                }
            };
            // iterate over the extensions and find the SNI
            for extension in res.1 {
                if let TlsExtension::SNI(sni) = extension {
                    // get the hostname
                    let hostname: &[u8] = sni[0].1;
                    let s: String = match String::from_utf8(hostname.to_vec()) {
                        Ok(v) => v,
                        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
                    };
                    return Some(s);
                }
            }
        }
        None
    }
}

impl ProxyDestinationDetector for TlsDestinationDetector {
    async fn determine(&self, stream: &mut TcpStream) -> anyhow::Result<ProxyDestination> {
        let mut buf = [0; 4096];
        let buf_len = stream.peek(&mut buf).await?;
        let domain = self.get_domain(&buf[..buf_len]).ok_or(anyhow!("domain not found"))?;
        Ok(ProxyDestination {
            domain,
            service: self.service,
            tls: true,
        })
    }
}
