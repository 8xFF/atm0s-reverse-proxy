use protocol::key::AgentSigner;
use rustls::{client::danger::ServerCertVerifier, pki_types::CertificateDer};
use serde::de::DeserializeOwned;
use std::{error::Error, net::ToSocketAddrs, sync::Arc, time::Duration};
use url::Url;

use quinn::{
    crypto::rustls::QuicClientConfig, ClientConfig, Endpoint, RecvStream, SendStream,
    TransportConfig,
};

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
        url: Url,
        agent_signer: &AS,
        server_certs: &[CertificateDer<'static>],
        allow_quic_insecure: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let url_host = url
            .host_str()
            .ok_or::<Box<dyn Error>>("couldn't get host from url".into())?;
        let url_port = url.port().unwrap_or(33333);
        log::info!("connecting to server {}:{}", url_host, url_port);
        let remote = (url_host, url_port)
            .to_socket_addrs()?
            .next()
            .ok_or::<Box<dyn Error>>("couldn't resolve to an address".into())?;

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().expect(""))?;
        endpoint.set_default_client_config(configure_client(server_certs, allow_quic_insecure)?);

        // connect to server
        let connection = endpoint.connect(remote, url_host)?.await?;

        log::info!("connected to {}, open bi stream", url);
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

fn configure_client(
    server_certs: &[CertificateDer],
    allow_quic_insecure: bool,
) -> Result<ClientConfig, Box<dyn Error>> {
    let mut config = if allow_quic_insecure {
        let provider = rustls::crypto::CryptoProvider::get_default().unwrap();
        ClientConfig::new(Arc::new(QuicClientConfig::try_from(
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(SkipServerVerification::new(provider.clone()))
                .with_no_client_auth(),
        )?))
    } else {
        let mut certs = rustls::RootCertStore::empty();
        for cert in server_certs {
            certs.add(cert.clone())?;
        }
        ClientConfig::with_root_certificates(Arc::new(certs))?
    };

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(3)));
    config.transport_config(Arc::new(transport));
    Ok(config)
}

#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Arc<Self> {
        Arc::new(Self(provider))
    }
}

impl ServerCertVerifier for SkipServerVerification {
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }

    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
}
