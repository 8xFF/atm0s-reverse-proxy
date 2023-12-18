use std::{error::Error, net::SocketAddr, sync::Arc};

use protocol::{
    key::validate_request,
    rpc::{RegisterRequest, RegisterResponse},
};
use quinn::{Endpoint, RecvStream, SendStream, ServerConfig};

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentQuicListener {
    endpoint: Endpoint,
    server_cert: Vec<u8>,
    root_domain: String,
}

impl AgentQuicListener {
    pub async fn new(addr: SocketAddr, root_domain: String) -> Self {
        log::info!("AgentQuicListener::new {}", addr);
        let (endpoint, server_cert) =
            make_server_endpoint(addr).expect("Should make server endpoint");

        Self {
            endpoint,
            server_cert,
            root_domain,
        }
    }

    async fn process_incoming_conn(&self, conn: quinn::Connection) -> Option<AgentQuicConnection> {
        let (mut send, mut recv) = conn.accept_bi().await.ok()?;
        let mut buf = [0u8; 4096];
        let buf_len = recv.read(&mut buf).await.ok()??;

        match RegisterRequest::try_from(&buf[..buf_len]) {
            Ok(request) => {
                let response = if let Some(sub_domain) = validate_request(&request) {
                    log::info!("register request domain {}", sub_domain);
                    Ok(format!("{}.{}", sub_domain, self.root_domain))
                } else {
                    log::error!("invalid register request {:?}", request);
                    Err(String::from("invalid request"))
                };

                let res = RegisterResponse { response };
                let res_buf: Vec<u8> = (&res).into();
                if let Err(e) = send.write_all(&res_buf).await {
                    log::error!("register response error {:?}", e);
                    return None;
                }

                let domain = res.response.ok()?;
                Some(AgentQuicConnection { domain, conn })
            }
            Err(e) => {
                log::error!("register request error {:?}", e);
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl AgentListener<AgentQuicConnection, AgentQuicSubConnection, RecvStream, SendStream>
    for AgentQuicListener
{
    async fn recv(&mut self) -> Option<AgentQuicConnection> {
        loop {
            let incoming_conn = self.endpoint.accept().await?;
            let conn: quinn::Connection = incoming_conn.await.ok()?;
            log::info!(
                "[AgentQuicListener] new conn from {}",
                conn.remote_address()
            );
            if let Some(connection) = self.process_incoming_conn(conn).await {
                return Some(connection);
            }
        }
    }
}

pub struct AgentQuicConnection {
    domain: String,
    conn: quinn::Connection,
}

#[async_trait::async_trait]
impl AgentConnection<AgentQuicSubConnection, RecvStream, SendStream> for AgentQuicConnection {
    fn domain(&self) -> String {
        self.domain.clone()
    }

    async fn create_sub_connection(&mut self) -> Option<AgentQuicSubConnection> {
        let (send, recv) = self.conn.open_bi().await.ok()?;
        Some(AgentQuicSubConnection { send, recv })
    }

    async fn recv(&mut self) -> Option<()> {
        self.conn.read_datagram().await.ok()?;
        Some(())
    }
}

pub struct AgentQuicSubConnection {
    send: SendStream,
    recv: RecvStream,
}

impl AgentSubConnection<RecvStream, SendStream> for AgentQuicSubConnection {
    fn split(self) -> (RecvStream, SendStream) {
        (self.recv, self.send)
    }
}

fn make_server_endpoint(bind_addr: SocketAddr) -> Result<(Endpoint, Vec<u8>), Box<dyn Error>> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> Result<(ServerConfig, Vec<u8>), Box<dyn Error>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    let priv_key = rustls::PrivateKey(priv_key);
    let cert_chain = vec![rustls::Certificate(cert_der.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}
