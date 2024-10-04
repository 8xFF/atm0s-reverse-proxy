use std::net::SocketAddr;

use clap::Parser;
use protocol::{
    DEFAULT_CLUSTER_CERT, DEFAULT_CLUSTER_KEY, DEFAULT_TUNNEL_CERT, DEFAULT_TUNNEL_KEY,
};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

/// A Certs util for quic, which generate der cert and key based on domain
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// UDP/TCP port for serving QUIC/TCP connection from agent
    #[arg(env, long, default_value = "0.0.0.0:33333")]
    agent_listener: SocketAddr,

    /// UDP/TCP port for serving QUIC/TCP connection for SDN network
    #[arg(env, long, default_value = "0.0.0.0:11111")]
    sdn_listener: SocketAddr,

    /// TCP port for serving HTTP connection
    #[arg(env, long, default_value = "0.0.0.0:80")]
    proxy_http_listener: SocketAddr,

    /// TCP port for serving TLS connection
    #[arg(env, long, default_value = "0.0.0.0:443")]
    proxy_tls_listener: SocketAddr,

    /// TCP port for serving TLS connection
    #[arg(env, long, default_value = "0.0.0.0:554")]
    proxy_rtsp_listener: SocketAddr,

    /// TCP port for serving TLS connection
    #[arg(env, long, default_value = "0.0.0.0:5543")]
    proxy_rtsps_listener: SocketAddr,
}

#[tokio::main]
async fn main() {
    let default_tunnel_cert = CertificateDer::from(DEFAULT_TUNNEL_CERT.to_vec());
    let default_tunnel_key = PrivatePkcs8KeyDer::from(DEFAULT_TUNNEL_KEY.to_vec());

    let default_cluster_cert = CertificateDer::from(DEFAULT_CLUSTER_CERT.to_vec());
    let default_cluster_key = PrivatePkcs8KeyDer::from(DEFAULT_CLUSTER_KEY.to_vec());
}
