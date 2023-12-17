use std::net::{Ipv4Addr, SocketAddr};

use async_std::net::{TcpListener, TcpStream};
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncReadExt,
};
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
impl ProxyListener<ProxyHttpTunnel, ReadHalf<TcpStream>, WriteHalf<TcpStream>>
    for ProxyHttpListener
{
    async fn recv(&mut self) -> Option<ProxyHttpTunnel> {
        let (stream, remote) = self.tcp_listener.accept().await.ok()?;
        log::info!("[ProxyHttpListener] new conn from {}", remote);
        Some(ProxyHttpTunnel {
            first_pkt: vec![0u8; 4096],
            first_pkt_size: 0,
            domain: "demo".to_string(),
            stream,
            tls: self.tls,
        })
    }
}

pub struct ProxyHttpTunnel {
    first_pkt: Vec<u8>,
    first_pkt_size: usize,
    domain: String,
    stream: TcpStream,
    tls: bool,
}

#[async_trait::async_trait]
impl ProxyTunnel<ReadHalf<TcpStream>, WriteHalf<TcpStream>> for ProxyHttpTunnel {
    fn first_pkt(&self) -> &[u8] {
        &self.first_pkt[..self.first_pkt_size]
    }

    async fn wait(&mut self) -> Option<()> {
        self.first_pkt_size = self.stream.read(&mut self.first_pkt).await.ok()?;
        log::info!(
            "[ProxyHttpTunnel] read {} bytes for determine url",
            self.first_pkt_size
        );
        if self.tls {
            self.domain = get_sni_from_packet(&self.first_pkt[..self.first_pkt_size])?;
        } else {
            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut req = httparse::Request::new(&mut headers);
            let _ = req.parse(&self.first_pkt[..self.first_pkt_size]).ok()?;
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
    fn split(self) -> (ReadHalf<TcpStream>, WriteHalf<TcpStream>) {
        AsyncReadExt::split(self.stream)
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
