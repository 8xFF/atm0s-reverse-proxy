use atm0s_sdn::{NodeAddr, NodeAliasSdk, NodeId};
use clap::Parser;
#[cfg(feature = "expose-metrics")]
use metrics_dashboard::build_dashboard_route;
#[cfg(feature = "expose-metrics")]
use poem::{listener::TcpListener, middleware::Tracing, EndpointExt as _, Route, Server};
use std::{collections::HashMap, net::SocketAddr, process::exit, sync::Arc};

use agent_listener::tcp::AgentTcpListener;
use agent_listener::{quic::AgentQuicListener, AgentSubConnection};
use async_std::sync::RwLock;
use futures::{select, AsyncRead, AsyncWrite, FutureExt};
use metrics::{
    decrement_gauge, describe_counter, describe_gauge, increment_counter, increment_gauge,
};
use proxy_listener::http::ProxyHttpListener;
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt, EnvFilter};

const METRICS_AGENT_COUNT: &str = "agent.count";
const METRICS_AGENT_LIVE: &str = "agent.live";
const METRICS_PROXY_COUNT: &str = "proxy.count";
const METRICS_CLUSTER_LIVE: &str = "cluster.live";
const METRICS_CLUSTER_COUNT: &str = "cluster.count";
pub(crate) const METRICS_PROXY_LIVE: &str = "proxy.live";

use crate::utils::domain_hash;
use crate::{
    agent_listener::{AgentConnection, AgentListener},
    proxy_listener::{ProxyListener, ProxyTunnel},
    tunnel::TunnelContext,
};

mod agent_listener;
mod agent_worker;
mod proxy_listener;
mod tunnel;
mod utils;

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

    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    let mut quic_agent_listener =
        AgentQuicListener::new(args.connector_port, args.root_domain.clone()).await;
    let mut tcp_agent_listener = AgentTcpListener::new(args.connector_port, args.root_domain).await;
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

    let (mut cluster_endpoint, node_alias_sdk, virtual_net) =
        proxy_listener::cluster::run_sdn(args.sdn_node_id, args.sdn_secret_key, args.sdn_seeds)
            .await;

    loop {
        select! {
            e = quic_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    run_agent_connection(agent_connection, agents.clone(), node_alias_sdk.clone()).await;
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = tcp_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    run_agent_connection(agent_connection, agents.clone(), node_alias_sdk.clone()).await;
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = proxy_http_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel::tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(node_alias_sdk.clone(), virtual_net.clone())));
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_tls_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel::tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(node_alias_sdk.clone(), virtual_net.clone())));
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = cluster_endpoint.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel::tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Cluster));
                }
                None => {
                    log::error!("cluster_endpoint.accept()");
                    exit(3);
                }
            }
        }
    }
}

async fn run_agent_connection<AG, S, R, W>(
    agent_connection: AG,
    agents: Arc<RwLock<HashMap<String, async_std::channel::Sender<Box<dyn ProxyTunnel>>>>>,
    node_alias_sdk: NodeAliasSdk,
) where
    AG: AgentConnection<S, R, W> + 'static,
    S: AgentSubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    increment_counter!(METRICS_AGENT_COUNT);
    log::info!("agent_connection.domain(): {}", agent_connection.domain());
    let domain = agent_connection.domain().to_string();
    let (mut agent_worker, proxy_tunnel_tx) =
        agent_worker::AgentWorker::<AG, S, R, W>::new(agent_connection);
    agents.write().await.insert(domain.clone(), proxy_tunnel_tx);
    node_alias_sdk.register(domain_hash(&domain));
    let agents = agents.clone();
    async_std::task::spawn(async move {
        increment_gauge!(METRICS_AGENT_LIVE, 1.0);
        log::info!("agent_worker run for domain: {}", domain);
        loop {
            match agent_worker.run().await {
                Ok(()) => {}
                Err(e) => {
                    log::error!("agent_worker error: {}", e);
                    break;
                }
            }
        }
        agents.write().await.remove(&domain);
        node_alias_sdk.unregister(domain_hash(&domain));
        log::info!("agent_worker exit for domain: {}", domain);
        decrement_gauge!(METRICS_AGENT_LIVE, 1.0);
    });
}
