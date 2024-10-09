use rustls::{client::danger::ServerCertVerifier, pki_types::CertificateDer};
use std::{sync::Arc, time::Duration};
use thiserror::Error;

use quinn::{
    crypto::rustls::{NoInitialCipherSuite, QuicClientConfig},
    ClientConfig, TransportConfig,
};

#[derive(Debug, Error)]
pub enum QuicTlsError {
    #[error("NoInitialCipherSuite")]
    NoInitialCipherSuite(#[from] NoInitialCipherSuite),
    #[error("Rustls")]
    Rustls(#[from] rustls::Error),
    #[error("VerifierBuilderError")]
    VerifierBuilderError(#[from] rustls::client::VerifierBuilderError),
}

pub fn configure_client(server_certs: &[CertificateDer], allow_quic_insecure: bool) -> Result<ClientConfig, QuicTlsError> {
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
    transport.keep_alive_interval(Some(Duration::from_secs(15)));
    transport.max_idle_timeout(Some(Duration::from_secs(30).try_into().expect("Should config timeout")));
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
    fn verify_tls12_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &rustls::DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }

    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
}
