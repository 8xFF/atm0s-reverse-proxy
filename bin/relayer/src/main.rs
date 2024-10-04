use std::net::SocketAddr;

use atm0s_reverse_proxy_relayer::{QuicRelayer, QuicRelayerConfig};
use clap::Parser;
use protocol::{DEFAULT_CLUSTER_CERT, DEFAULT_CLUSTER_KEY, DEFAULT_TUNNEL_CERT, DEFAULT_TUNNEL_KEY};
use protocol_ed25519::ClusterValidatorImpl;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// A Certs util for quic, which generate der cert and key based on domain
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Root domain for generate an unique domain for each agent
    #[arg(env, long, default_value = "local.ha.8xff.io")]
    root_domain: String,

    /// UDP/TCP port for serving QUIC/TCP connection from agent
    #[arg(env, long, default_value = "0.0.0.0:33333")]
    agent_listener: SocketAddr,

    /// UDP/TCP port for serving QUIC/TCP connection for SDN network
    #[arg(env, long, default_value = "0.0.0.0:11111")]
    sdn_listener: SocketAddr,

    /// Seeds
    #[arg(env, long, value_delimiter = ',')]
    sdn_seeds: Vec<SocketAddr>,

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
    rustls::crypto::ring::default_provider().install_default().expect("should install ring as default");

    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }
    let args: Args = Args::parse();
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    let default_tunnel_cert = CertificateDer::from(DEFAULT_TUNNEL_CERT.to_vec());
    let default_tunnel_key = PrivatePkcs8KeyDer::from(DEFAULT_TUNNEL_KEY.to_vec());

    let default_cluster_cert = CertificateDer::from(DEFAULT_CLUSTER_CERT.to_vec());
    let default_cluster_key = PrivatePkcs8KeyDer::from(DEFAULT_CLUSTER_KEY.to_vec());

    let cfg = QuicRelayerConfig {
        agent_listener: args.agent_listener,
        sdn_listener: args.sdn_listener,
        proxy_http_listener: args.proxy_http_listener,
        proxy_tls_listener: args.proxy_tls_listener,
        proxy_rtsp_listener: args.proxy_rtsp_listener,
        proxy_rtsps_listener: args.proxy_rtsps_listener,
        agent_key: default_tunnel_key,
        agent_cert: default_tunnel_cert,
        sdn_key: default_cluster_key,
        sdn_cert: default_cluster_cert,
        sdn_seeds: args.sdn_seeds.into_iter().map(|a| a.into()).collect::<Vec<_>>(),
    };
    let validator = ClusterValidatorImpl::new(args.root_domain);
    let mut relayer = QuicRelayer::new(cfg, validator).await.expect("should create relayer");
    while let Ok(_) = relayer.recv().await {}
}
