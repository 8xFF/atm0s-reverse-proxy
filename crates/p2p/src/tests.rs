use std::net::UdpSocket;

use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

use crate::{P2pNetwork, P2pNetworkConfig, PeerAddress};

mod cross_nodes;
mod discovery;

pub const DEFAULT_CLUSTER_CERT: &[u8] = include_bytes!("../certs/dev.cluster.cert");
pub const DEFAULT_CLUSTER_KEY: &[u8] = include_bytes!("../certs/dev.cluster.key");

async fn create_random_node(advertise: bool) -> (P2pNetwork, PeerAddress) {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let key: PrivatePkcs8KeyDer<'_> = PrivatePkcs8KeyDer::from(DEFAULT_CLUSTER_KEY.to_vec());
    let cert = CertificateDer::from(DEFAULT_CLUSTER_CERT.to_vec());

    let addr = {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.local_addr().unwrap()
    };
    (
        P2pNetwork::new(P2pNetworkConfig {
            addr,
            advertise: advertise.then(|| addr.into()),
            priv_key: key,
            cert: cert,
        })
        .await
        .unwrap(),
        addr.into(),
    )
}
