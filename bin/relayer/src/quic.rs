use std::{net::SocketAddr, sync::Arc, time::Duration};

use protocol::stream::TunnelStream;
use quinn::{ClientConfig, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

pub type TunnelQuicStream = TunnelStream<RecvStream, SendStream>;

pub fn make_server_endpoint(bind_addr: SocketAddr, priv_key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>) -> anyhow::Result<Endpoint> {
    let server_config = configure_server(priv_key, cert.clone())?;
    let client_config = configure_client(&[cert])?;
    let mut endpoint = Endpoint::server(server_config, bind_addr)?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

/// Returns default server configuration along with its certificate.
fn configure_server(priv_key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>) -> anyhow::Result<ServerConfig> {
    let cert_chain = vec![cert];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key.into())?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(30).try_into().expect("Should config timeout")));

    Ok(server_config)
}

fn configure_client(server_certs: &[CertificateDer]) -> anyhow::Result<ClientConfig> {
    let mut certs = rustls::RootCertStore::empty();
    for cert in server_certs {
        certs.add(cert.clone())?;
    }
    let mut config = ClientConfig::with_root_certificates(Arc::new(certs))?;

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(15)));
    transport.max_idle_timeout(Some(Duration::from_secs(30).try_into().expect("Should config timeout")));
    config.transport_config(Arc::new(transport));
    Ok(config)
}
