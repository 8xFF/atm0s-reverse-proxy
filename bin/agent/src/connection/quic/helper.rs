use rustls::pki_types::CertificateDer;
use std::{sync::Arc, time::Duration};
use thiserror::Error;

use quinn::{
    crypto::rustls::{NoInitialCipherSuite, QuicClientConfig},
    ClientConfig, TransportConfig,
};

use super::no_servername_verify::NoServerNameVerification;

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
    let mut root = rustls::RootCertStore::empty();
    for cert in server_certs {
        root.add(cert.clone())?;
    }

    let client = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoServerNameVerification::new(root, allow_quic_insecure)?))
        .with_no_client_auth();
    let mut config = ClientConfig::new(Arc::new(QuicClientConfig::try_from(client)?));

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(15)));
    transport.max_idle_timeout(Some(Duration::from_secs(30).try_into().expect("Should config timeout")));
    config.transport_config(Arc::new(transport));
    Ok(config)
}
