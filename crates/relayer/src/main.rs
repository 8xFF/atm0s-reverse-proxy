use atm0s_sdn::{NodeAddr, NodeId};
use clap::Parser;
#[cfg(feature = "expose-metrics")]
use metrics_dashboard::build_dashboard_route;
#[cfg(feature = "expose-metrics")]
use poem::{listener::TcpListener, middleware::Tracing, EndpointExt as _, Route, Server};
use relayer::{
    run_agent_connection, run_sdn, tunnel_task, AgentListener, AgentQuicListener,
    AgentRpcHandlerDummy, AgentTcpListener, ProxyHttpListener, ProxyListener, TunnelContext,
    METRICS_AGENT_COUNT, METRICS_AGENT_LIVE, METRICS_PROXY_COUNT, METRICS_PROXY_LIVE,
};
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
}

#[async_std::main]
async fn main() {
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
    let mut quic_agent_listener =
        AgentQuicListener::new(args.connector_port, cluster_validator.clone()).await;
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

    let (mut cluster_endpoint, node_alias_sdk, virtual_net, _rpc_box) = run_sdn(
        args.sdn_node_id,
        args.sdn_port,
        args.sdn_secret_key,
        args.sdn_seeds,
    )
    .await;

    let agent_rpc_handler = Arc::new(AgentRpcHandlerDummy::default());

    loop {
        select! {
            e = quic_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    run_agent_connection(agent_connection, agents.clone(), node_alias_sdk.clone(), agent_rpc_handler.clone()).await;
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = tcp_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    run_agent_connection(agent_connection, agents.clone(), node_alias_sdk.clone(), agent_rpc_handler.clone()).await;
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = proxy_http_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(node_alias_sdk.clone(), virtual_net.clone())));
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_tls_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(node_alias_sdk.clone(), virtual_net.clone())));
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
            }
        }
    }
}
