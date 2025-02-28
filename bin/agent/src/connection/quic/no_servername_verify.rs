// this code is copy from https://gist.github.com/doroved/2c92ddd5e33f257f901c763b728d1b61
use rustls::{
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        WebPkiServerVerifier,
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::VerifierBuilderError,
    DigitallySignedStruct, RootCertStore,
};
use std::sync::Arc;

#[derive(Debug)]
pub struct NoServerNameVerification {
    inner: Arc<WebPkiServerVerifier>,
    allow_quic_insecure: bool,
}

impl NoServerNameVerification {
    pub fn new(root: RootCertStore, allow_quic_insecure: bool) -> Result<Self, VerifierBuilderError> {
        Ok(Self {
            inner: rustls::client::WebPkiServerVerifier::builder(Arc::new(root)).build()?,
            allow_quic_insecure,
        })
    }
}

impl ServerCertVerifier for NoServerNameVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if self.allow_quic_insecure {
            return Ok(ServerCertVerified::assertion());
        }

        match self.inner.verify_server_cert(_end_entity, _intermediates, _server_name, _ocsp, _now) {
            Ok(scv) => Ok(scv),
            Err(rustls::Error::InvalidCertificate(cert_error)) => {
                if let rustls::CertificateError::NotValidForName = cert_error {
                    Ok(ServerCertVerified::assertion())
                } else {
                    Err(rustls::Error::InvalidCertificate(cert_error))
                }
            }
            Err(e) => Err(e),
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}
