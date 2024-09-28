use metrics::{counter, gauge};
use proxy_listener::ProxyTunnelWrap;

use crate::utils::home_id_from_domain;

// this is for online agent counting
pub const METRICS_AGENT_LIVE: &str = "atm0s_agent_live";
pub const METRICS_AGENT_HISTOGRAM: &str = "atm0s_agent_histogram";
pub const METRICS_AGENT_COUNT: &str = "atm0s_agent_count";

// this is for proxy from agent counting (incoming)
pub const METRICS_PROXY_AGENT_LIVE: &str = "atm0s_proxy_agent_live";
pub const METRICS_PROXY_AGENT_COUNT: &str = "atm0s_proxy_agent_count";
pub const METRICS_PROXY_AGENT_HISTOGRAM: &str = "atm0s_proxy_agent_histogram";
pub const METRICS_PROXY_AGENT_ERROR_COUNT: &str = "atm0s_proxy_agent_error_count";

// this is for http proxy counting (incoming)
pub const METRICS_PROXY_HTTP_LIVE: &str = "atm0s_proxy_http_live";
pub const METRICS_PROXY_HTTP_COUNT: &str = "atm0s_proxy_http_count";
pub const METRICS_PROXY_HTTP_ERROR_COUNT: &str = "atm0s_proxy_http_error_count";

// this is for cluster proxy (incoming)
pub const METRICS_PROXY_CLUSTER_LIVE: &str = "atm0s_proxy_cluster_live";
pub const METRICS_PROXY_CLUSTER_COUNT: &str = "atm0s_proxy_cluster_count";
pub const METRICS_PROXY_CLUSTER_ERROR_COUNT: &str = "atm0s_proxy_cluster_error_count";

// this is for tunnel from local node to other node (outgoing)
pub const METRICS_TUNNEL_CLUSTER_LIVE: &str = "atm0s_tunnel_cluster_live";
pub const METRICS_TUNNEL_CLUSTER_COUNT: &str = "atm0s_tunnel_cluster_count";
pub const METRICS_TUNNEL_CLUSTER_HISTOGRAM: &str = "atm0s_tunnel_cluster_histogram";
pub const METRICS_TUNNEL_CLUSTER_ERROR_COUNT: &str = "atm0s_tunnel_cluster_error_count";

// this is for tunnel from local node to agent  (outgoing)
pub const METRICS_TUNNEL_AGENT_LIVE: &str = "atm0s_tunnel_agent_live";
pub const METRICS_TUNNEL_AGENT_COUNT: &str = "atm0s_tunnel_agent_count";
pub const METRICS_TUNNEL_AGENT_HISTOGRAM: &str = "atm0s_tunnel_agent_histogram";
pub const METRICS_TUNNEL_AGENT_ERROR_COUNT: &str = "atm0s_tunnel_agent_error_count";

mod agent_listener;
mod agent_store;
mod agent_worker;
mod proxy_listener;
mod tunnel;
mod utils;

pub use agent_listener::quic::{AgentQuicConnection, AgentQuicListener, AgentQuicSubConnection};
pub use agent_listener::tcp::{AgentTcpConnection, AgentTcpListener, AgentTcpSubConnection};
pub use agent_listener::{
    AgentConnection, AgentConnectionHandler, AgentIncomingConnHandlerDummy, AgentListener,
    AgentSubConnection,
};
pub use atm0s_sdn;
pub use proxy_listener::cluster::{
    make_quinn_client, make_quinn_server, AliasSdk, VirtualNetwork, VirtualUdpSocket,
};
pub use quinn;

pub use proxy_listener::cluster::{run_sdn, ProxyClusterIncommingTunnel, ProxyClusterListener};
pub use proxy_listener::tcp::{ProxyTcpListener, ProxyTcpTunnel};
pub use proxy_listener::{ProxyListener, ProxyTunnel};

pub use agent_store::AgentStore;
pub use proxy_listener::tcp::{HttpDomainDetector, RtspDomainDetector, TlsDomainDetector};
pub use tunnel::{tunnel_task, TunnelContext};

pub async fn run_agent_connection<AG, S, H>(
    agent_connection: AG,
    agents: AgentStore,
    node_alias_sdk: AliasSdk,
    agent_rpc_handler: H,
) where
    AG: AgentConnection<S> + 'static,
    S: AgentSubConnection + 'static,
    H: AgentConnectionHandler<S> + 'static,
{
    counter!(METRICS_AGENT_COUNT).increment(1);
    log::info!("agent_connection.domain(): {}", agent_connection.domain());
    let domain = agent_connection.domain().to_string();
    let (mut agent_worker, proxy_tunnel_tx) =
        agent_worker::AgentWorker::<AG, ProxyTunnelWrap, S, H>::new(
            agent_connection,
            agent_rpc_handler,
        );
    let home_id = home_id_from_domain(&domain);
    agents.add(home_id, proxy_tunnel_tx);
    node_alias_sdk.register_alias(home_id).await;
    let agents = agents.clone();
    gauge!(METRICS_AGENT_LIVE).increment(1.0);
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
    agents.remove(home_id);
    node_alias_sdk
        .unregister_alias(home_id_from_domain(&domain))
        .await;
    log::info!("agent_worker exit for domain: {}", domain);
    gauge!(METRICS_AGENT_LIVE).decrement(1.0);
}
