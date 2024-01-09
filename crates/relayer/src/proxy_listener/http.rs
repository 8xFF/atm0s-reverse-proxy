use std::net::{Ipv4Addr, SocketAddr};

use async_std::net::{TcpListener, TcpStream};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite};
use tls_parser::{parse_tls_extensions, parse_tls_plaintext};

use super::{ProxyListener, ProxyTunnel};

pub struct ProxyHttpListener {
    tcp_listener: TcpListener,
    tls: bool,
}

impl ProxyHttpListener {
    pub async fn new(port: u16, tls: bool) -> Option<Self> {
        log::info!("ProxyHttpListener::new {}", port);
        Some(Self {
            tcp_listener: TcpListener::bind(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port))
                .await
                .ok()?,
            tls,
        })
    }
}

#[async_trait::async_trait]
impl ProxyListener for ProxyHttpListener {
    async fn recv(&mut self) -> Option<Box<dyn ProxyTunnel>> {
        let (stream, remote) = self.tcp_listener.accept().await.ok()?;
        log::info!("[ProxyHttpListener] new conn from {}", remote);
        Some(Box::new(ProxyHttpTunnel {
            domain: "".to_string(),
            stream: Some(stream),
            tls: self.tls,
        }))
    }
}

pub struct ProxyHttpTunnel {
    domain: String,
    stream: Option<TcpStream>,
    tls: bool,
}

#[async_trait::async_trait]
impl ProxyTunnel for ProxyHttpTunnel {
    async fn wait(&mut self) -> Option<()> {
        log::info!("[ProxyHttpTunnel] wait first data for checking url...");
        let mut first_pkt = [0u8; 4096];
        let stream = self.stream.as_mut()?;
        let first_pkt_size = stream.peek(&mut first_pkt).await.ok()?;
        log::info!(
            "[ProxyHttpTunnel] read {} bytes for determine url",
            first_pkt_size
        );
        if self.tls {
            self.domain = get_sni_from_packet(&first_pkt[..first_pkt_size])?;
        } else {
            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut req = httparse::Request::new(&mut headers);
            let _ = req.parse(&first_pkt[..first_pkt_size]).ok()?;
            let domain = req.headers.iter().find(|h| h.name == "Host")?.value;
            // dont get the port
            let domain = String::from_utf8_lossy(domain).to_string();
            let domain = domain.split(':').next()?;
            self.domain = domain.to_string();
        }
        Some(())
    }

    fn domain(&self) -> &str {
        &self.domain
    }
    fn split(
        &mut self,
    ) -> (
        Box<dyn AsyncRead + Send + Sync + Unpin>,
        Box<dyn AsyncWrite + Send + Sync + Unpin>,
    ) {
        let (read, write) = AsyncReadExt::split(self.stream.take().expect("Should has stream"));
        (Box::new(read), Box::new(write))
    }
}

fn get_sni_from_packet(packet: &[u8]) -> Option<String> {
    let res = match parse_tls_plaintext(&packet) {
        Ok(res) => res,
        Err(e) => {
            log::error!("parse_tls_plaintext error {:?}", e);
            return None;
        }
    };

    let tls_message = &res.1.msg[0];
    if let tls_parser::TlsMessage::Handshake(handshake) = tls_message {
        if let tls_parser::TlsMessageHandshake::ClientHello(client_hello) = handshake {
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
                if let tls_parser::TlsExtension::SNI(sni) = extension {
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
    }
    None
}
