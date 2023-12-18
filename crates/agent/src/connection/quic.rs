use std::sync::Arc;
use std::time::Duration;
use std::{error::Error, net::SocketAddr};

use protocol::{key::LocalKey, rpc::RegisterResponse};
use quinn::{ClientConfig, Endpoint, RecvStream, SendStream, TransportConfig};

use super::{Connection, SubConnection};

pub struct QuicSubConnection {
    pub send: SendStream,
    pub recv: RecvStream,
}

impl SubConnection<RecvStream, SendStream> for QuicSubConnection {
    fn split(self) -> (RecvStream, SendStream) {
        (self.recv, self.send)
    }
}

pub struct QuicConnection {
    connection: quinn::Connection,
    #[allow(unused)]
    domain: String,
}

impl QuicConnection {
    pub async fn new(dest: SocketAddr, local_key: &LocalKey) -> Result<Self, Box<dyn Error>> {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().expect(""))?;
        endpoint.set_default_client_config(configure_client());

        // connect to server
        let connection = endpoint.connect(dest, "localhost")?.await?;

        log::info!("connected to {}, open bi stream", dest);
        let (mut send_stream, mut recv_stream) = connection.open_bi().await?;
        log::info!("opened bi stream, send register request");

        let request = local_key.to_request();
        let request_buf: Vec<u8> = (&request).into();
        send_stream.write_all(&request_buf).await?;

        let mut buf = [0u8; 4096];
        let buf_len = recv_stream
            .read(&mut buf)
            .await?
            .ok_or::<Box<dyn Error>>("read register response error".into())?;
        let response = RegisterResponse::try_from(&buf[..buf_len])?;
        match response.response {
            Ok(domain) => {
                log::info!("registed domain {}", domain);
                Ok(Self { connection, domain })
            }
            Err(e) => {
                log::error!("register response error {}", e);
                return Err(e.into());
            }
        }
    }
}

#[async_trait::async_trait]
impl Connection<QuicSubConnection, RecvStream, SendStream> for QuicConnection {
    async fn recv(&mut self) -> Result<QuicSubConnection, Box<dyn Error>> {
        let (send, recv) = self.connection.accept_bi().await?;
        Ok(QuicSubConnection { send, recv })
    }
}

fn configure_client() -> ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(5)));

    let mut config = ClientConfig::new(Arc::new(crypto) as Arc<_>);
    config.transport_config(Arc::new(transport));
    config
}

/// Dummy certificate verifier that treats any certificate as valid.
/// NOTE, such verification is vulnerable to MITM attacks, but convenient for testing.
struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
