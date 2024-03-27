use protocol::key::AgentSigner;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use serde::de::DeserializeOwned;
use std::{error::Error, net::SocketAddr, sync::Arc, time::Duration};

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

pub struct QuicConnection<RES> {
    response: RES,
    connection: quinn::Connection,
}

impl<RES: DeserializeOwned> QuicConnection<RES> {
    pub async fn new<AS: AgentSigner<RES>>(
        dest: SocketAddr,
        agent_signer: &AS,
    ) -> Result<Self, Box<dyn Error>> {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().expect(""))?;
        endpoint.set_default_client_config(configure_client());

        // connect to server
        let connection = endpoint.connect(dest, "localhost")?.await?;

        log::info!("connected to {}, open bi stream", dest);
        let (mut send_stream, mut recv_stream) = connection.open_bi().await?;
        log::info!("opened bi stream, send register request");

        send_stream
            .write_all(&agent_signer.sign_connect_req())
            .await?;

        let mut buf = [0u8; 4096];
        let buf_len = recv_stream
            .read(&mut buf)
            .await?
            .ok_or::<Box<dyn Error>>("read register response error".into())?;
        let response: RES = agent_signer.validate_connect_res(&buf[..buf_len])?;
        Ok(Self {
            connection,
            response,
        })
    }

    pub fn response(&self) -> &RES {
        &self.response
    }
}

#[async_trait::async_trait]
impl<RES: Send + Sync> Connection<QuicSubConnection, RecvStream, SendStream>
    for QuicConnection<RES>
{
    async fn create_outgoing(&mut self) -> Result<QuicSubConnection, Box<dyn Error>> {
        let (send, recv) = self.connection.open_bi().await?;
        Ok(QuicSubConnection { send, recv })
    }

    async fn recv(&mut self) -> Result<QuicSubConnection, Box<dyn Error>> {
        let (send, recv) = self.connection.accept_bi().await?;
        Ok(QuicSubConnection { send, recv })
    }
}

/// Dummy certificate verifier that treats any certificate as valid.
/// NOTE, such verification is vulnerable to MITM attacks, but convenient for testing.
#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Arc<Self> {
        Arc::new(Self(provider))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn configure_client() -> ClientConfig {
    let provider =
        rustls::crypto::CryptoProvider::get_default().expect("should create crypto provider");
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(SkipServerVerification::new(provider.clone()))
        .with_no_client_auth();

    let mut config = ClientConfig::new(Arc::new(crypto));
    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(3)));
    config.transport_config(Arc::new(transport));
    config
}
