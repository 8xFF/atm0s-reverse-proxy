#[cfg(feature = "expose-metrics")]
use metrics_dashboard::build_dashboard_route;
#[cfg(feature = "expose-metrics")]
use poem::{listener::TcpListener, middleware::Tracing, EndpointExt as _, Route, Server};
use std::sync::Arc;

use futures::{AsyncRead, AsyncWrite};
use metrics::{counter, gauge};

use crate::utils::home_id_from_domain;

pub const METRICS_AGENT_COUNT: &str = "atm0s_proxy_agent_count";
pub const METRICS_AGENT_LIVE: &str = "atm0s_proxy_agent_live";
pub const METRICS_PROXY_COUNT: &str = "atm0s_proxy_proxy_count";
pub const METRICS_CLUSTER_LIVE: &str = "atm0s_proxy_cluster_live";
pub const METRICS_CLUSTER_COUNT: &str = "atm0s_proxy_cluster_count";
pub const METRICS_PROXY_LIVE: &str = "atm0s_proxy_proxy_live";

mod agent_listener;
mod agent_store;
mod agent_worker;
mod proxy_listener;
mod tunnel;
mod utils;

pub use agent_listener::quic::{AgentQuicConnection, AgentQuicListener, AgentQuicSubConnection};
pub use agent_listener::tcp::{AgentTcpConnection, AgentTcpListener, AgentTcpSubConnection};
pub use agent_listener::{
    AgentConnection, AgentConnectionHandler, AgentIncommingConnHandlerDummy, AgentListener,
    AgentSubConnection,
};
pub use atm0s_sdn;
pub use proxy_listener::cluster::{
    make_quinn_client, make_quinn_server, AliasSdk, VirtualNetwork, VirtualUdpSocket,
};
pub use quinn;

pub use proxy_listener::cluster::{run_sdn, ProxyClusterListener, ProxyClusterTunnel};
pub use proxy_listener::http::{ProxyHttpListener, ProxyHttpTunnel};
pub use proxy_listener::{ProxyListener, ProxyTunnel};

pub use agent_store::AgentStore;
pub use tunnel::{tunnel_task, TunnelContext};

pub async fn run_agent_connection<AG, S, R, W>(
    agent_connection: AG,
    agents: AgentStore,
    node_alias_sdk: AliasSdk,
    agent_rpc_handler: Arc<dyn AgentConnectionHandler<S, R, W>>,
) where
    AG: AgentConnection<S, R, W> + 'static,
    S: AgentSubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    counter!(METRICS_AGENT_COUNT).increment(1);
    log::info!("agent_connection.domain(): {}", agent_connection.domain());
    let domain = agent_connection.domain().to_string();
    let (mut agent_worker, proxy_tunnel_tx) =
        agent_worker::AgentWorker::<AG, S, R, W>::new(agent_connection, agent_rpc_handler);
    let home_id = home_id_from_domain(&domain);
    agents.add(home_id.clone(), proxy_tunnel_tx);
    node_alias_sdk.register_alias(home_id.clone()).await;
    let agents = agents.clone();
    async_std::task::spawn(async move {
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
    });
}
