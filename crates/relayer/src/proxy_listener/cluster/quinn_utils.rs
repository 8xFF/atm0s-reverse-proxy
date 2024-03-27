use quinn::{
    AsyncStdRuntime, ClientConfig, Endpoint, EndpointConfig, ServerConfig, TransportConfig,
};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use super::vsocket::VirtualUdpSocket;

pub fn make_insecure_quinn_server(socket: VirtualUdpSocket) -> Result<Endpoint, Box<dyn Error>> {
    let (server_config, _server_cert) = configure_server()?;
    let runtime = Arc::new(AsyncStdRuntime);
    let mut config = EndpointConfig::default();
    config
        .max_udp_payload_size(1500)
        .expect("Should config quinn server max_size to 1500");
    Endpoint::new_with_abstract_socket(config, Some(server_config), Arc::new(socket), runtime)
        .map_err(|e| e.into())
}

pub fn make_insecure_quinn_client(socket: VirtualUdpSocket) -> Result<Endpoint, std::io::Error> {
    let runtime = Arc::new(AsyncStdRuntime);
    let mut config = EndpointConfig::default();
    //Note that client mtu size shoud be smaller than server's
    config
        .max_udp_payload_size(1400)
        .expect("Should config quinn client max_size to 1400");
    let mut endpoint = Endpoint::new_with_abstract_socket(config, None, Arc::new(socket), runtime)?;
    endpoint.set_default_client_config(configure_client());
    Ok(endpoint)
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> Result<(ServerConfig, Vec<u8>), Box<dyn Error>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    let priv_key = PrivatePkcs8KeyDer::from(priv_key);
    let cert_chain = vec![CertificateDer::from(cert_der.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key.into())?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
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
        rustls::crypto::CryptoProvider::get_default().expect("should have crypto provider");
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
