use atm0s_reverse_proxy_relayer::{
    run_agent_connection, run_sdn, tunnel_task, AgentIncommingConnHandlerDummy, AgentListener,
    AgentQuicListener, AgentTcpListener, ProxyHttpListener, ProxyListener, TunnelContext,
    METRICS_AGENT_COUNT, METRICS_AGENT_LIVE, METRICS_PROXY_COUNT, METRICS_PROXY_LIVE,
};
use atm0s_sdn::{NodeAddr, NodeId};
use clap::Parser;
#[cfg(feature = "expose-metrics")]
use metrics_dashboard::build_dashboard_route;
#[cfg(feature = "expose-metrics")]
use poem::{listener::TcpListener, middleware::Tracing, EndpointExt as _, Route, Server};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{collections::HashMap, net::SocketAddr, process::exit, sync::Arc};

use async_std::sync::RwLock;
use futures::{select, FutureExt};
use metrics::{describe_counter, describe_gauge};
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt, EnvFilter};

/// A HTTP and SNI HTTPs proxy for expose your local service to the internet.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// API port
    #[arg(env, long, default_value_t = 33334)]
    api_port: u16,

    /// Http proxy port
    #[arg(env, long, default_value_t = 80)]
    http_port: u16,

    /// Sni-https proxy port
    #[arg(env, long, default_value_t = 443)]
    https_port: u16,

    /// Number of times to greet
    #[arg(env, long, default_value = "0.0.0.0:33333")]
    connector_port: SocketAddr,

    /// Root domain
    #[arg(env, long, default_value = "localtunnel.me")]
    root_domain: String,

    /// atm0s-sdn node-id
    #[arg(env, long)]
    sdn_node_id: NodeId,

    /// atm0s-sdn node-id
    #[arg(env, long, default_value_t = 0)]
    sdn_port: u16,

    /// atm0s-sdn secret key
    #[arg(env, long, default_value = "insecure")]
    sdn_secret_key: String,

    /// atm0s-sdn seed address
    #[arg(env, long)]
    sdn_seeds: Vec<NodeAddr>,

    /// atm0s-sdn workers
    #[arg(env, long, default_value_t = 1)]
    sdn_workers: usize,
}

#[async_std::main]
async fn main() {
    let default_tunnel_cert_buf = include_bytes!("../../../certs/tunnel.cert");
    let default_tunnel_key_buf = include_bytes!("../../../certs/tunnel.key");
    let default_tunnel_cert = CertificateDer::from(default_tunnel_cert_buf.to_vec());
    let default_tunnel_key = PrivatePkcs8KeyDer::from(default_tunnel_key_buf.to_vec());

    let default_cluster_cert_buf = include_bytes!("../../../certs/cluster.cert");
    let default_cluster_key_buf = include_bytes!("../../../certs/cluster.key");
    let default_cluster_cert = CertificateDer::from(default_cluster_cert_buf.to_vec());
    let default_cluster_key = PrivatePkcs8KeyDer::from(default_cluster_key_buf.to_vec());

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("should install ring as default");
    let args = Args::parse();
    let cluster_validator = protocol_ed25519::ClusterValidatorImpl::new(args.root_domain);

    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    let mut quic_agent_listener = AgentQuicListener::new(
        args.connector_port,
        cluster_validator.clone(),
        default_tunnel_key,
        default_tunnel_cert,
    )
    .await;
    let mut tcp_agent_listener =
        AgentTcpListener::new(args.connector_port, cluster_validator).await;
    let mut proxy_http_listener = ProxyHttpListener::new(args.http_port, false)
        .await
        .expect("Should listen http port");
    let mut proxy_tls_listener = ProxyHttpListener::new(args.https_port, true)
        .await
        .expect("Should listen tls port");
    let agents = Arc::new(RwLock::new(HashMap::new()));

    #[cfg(feature = "expose-metrics")]
    let app = Route::new()
        .nest("/dashboard/", build_dashboard_route())
        .with(Tracing);

    describe_counter!(METRICS_AGENT_COUNT, "Sum agent connect count");
    describe_gauge!(METRICS_AGENT_LIVE, "Live agent count");
    describe_counter!(METRICS_PROXY_COUNT, "Sum proxy connect count");
    describe_gauge!(METRICS_PROXY_LIVE, "Live proxy count");

    #[cfg(feature = "expose-metrics")]
    async_std::task::spawn(async move {
        let _ = Server::new(TcpListener::bind("0.0.0.0:33334"))
            .name("relay-metrics")
            .run(app)
            .await;
    });

    let (mut cluster_endpoint, alias_sdk, mut virtual_net) = run_sdn(
        args.sdn_node_id,
        args.sdn_port,
        args.sdn_secret_key,
        args.sdn_seeds,
        args.sdn_workers,
        default_cluster_key,
        default_cluster_cert.clone(),
    )
    .await;

    let agent_rpc_handler_quic = Arc::new(AgentIncommingConnHandlerDummy::default());
    let agent_rpc_handler_tcp = Arc::new(AgentIncommingConnHandlerDummy::default());

    loop {
        select! {
            e = quic_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    run_agent_connection(agent_connection, agents.clone(), alias_sdk.clone(), agent_rpc_handler_quic.clone()).await;
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = tcp_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    run_agent_connection(agent_connection, agents.clone(), alias_sdk.clone(), agent_rpc_handler_tcp.clone()).await;
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = proxy_http_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    if let Some(socket) = virtual_net.udp_socket(0).await {
                        async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(alias_sdk.clone(), socket, vec![default_cluster_cert.clone()])));
                    } else {
                        log::error!("Virtual Net create socket error");
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_tls_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    if let Some(socket) = virtual_net.udp_socket(0).await {
                        async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(alias_sdk.clone(), socket, vec![default_cluster_cert.clone()])));
                    } else {
                        log::error!("Virtual Net create socket error");
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = cluster_endpoint.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Cluster));
                }
                None => {
                    log::error!("cluster_endpoint.accept()");
                    exit(3);
                }
            },
            e = virtual_net.recv().fuse() => match e {
                Some(()) => {}
                None => {
                    log::error!("virtual_net.recv()");
                    exit(4);
                }
            }
        }
    }
}
