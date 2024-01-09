use std::{error::Error, net::SocketAddr, sync::Arc};

use protocol::{
    key::validate_request,
    rpc::{RegisterRequest, RegisterResponse},
};
use quinn::{Endpoint, RecvStream, SendStream, ServerConfig};

use super::{AgentConnection, AgentListener, AgentSubConnection};

pub struct AgentQuicListener {
    endpoint: Endpoint,
    root_domain: String,
}

impl AgentQuicListener {
    pub async fn new(addr: SocketAddr, root_domain: String) -> Self {
        log::info!("AgentQuicListener::new {}", addr);
        let (endpoint, _server_cert) =
            make_server_endpoint(addr).expect("Should make server endpoint");

        Self {
            endpoint,
            root_domain,
        }
    }

    async fn process_incoming_conn(
        &self,
        conn: quinn::Connection,
    ) -> Result<AgentQuicConnection, Box<dyn Error>> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let mut buf = [0u8; 4096];
        let buf_len = recv
            .read(&mut buf)
            .await?
            .ok_or::<Box<dyn Error>>("No incomming data".into())?;

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
                send.write_all(&res_buf).await?;

                let domain = res.response?;
                Ok(AgentQuicConnection { domain, conn })
            }
            Err(e) => {
                log::error!("register request error {:?}", e);
                Err(e.into())
            }
        }
    }
}

#[async_trait::async_trait]
impl AgentListener<AgentQuicConnection, AgentQuicSubConnection, RecvStream, SendStream>
    for AgentQuicListener
{
    async fn recv(&mut self) -> Result<AgentQuicConnection, Box<dyn Error>> {
        loop {
            let incoming_conn = self
                .endpoint
                .accept()
                .await
                .ok_or::<Box<dyn Error>>("Cannot accept".into())?;
            let conn: quinn::Connection = incoming_conn.await?;
            log::info!(
                "[AgentQuicListener] new conn from {}",
                conn.remote_address()
            );
            match self.process_incoming_conn(conn).await {
                Ok(connection) => {
                    log::info!("new connection {}", connection.domain());
                    return Ok(connection);
                }
                Err(e) => {
                    log::error!("process_incoming_conn error: {}", e);
                }
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

    async fn create_sub_connection(&mut self) -> Result<AgentQuicSubConnection, Box<dyn Error>> {
        let (send, recv) = self.conn.open_bi().await?;
        Ok(AgentQuicSubConnection { send, recv })
    }

    async fn recv(&mut self) -> Result<(), Box<dyn Error>> {
        self.conn.read_datagram().await?;
        Ok(())
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
