use atm0s_sdn::{NodeAliasId, NodeAliasSdk};
#[cfg(feature = "expose-metrics")]
use metrics_dashboard::build_dashboard_route;
#[cfg(feature = "expose-metrics")]
use poem::{listener::TcpListener, middleware::Tracing, EndpointExt as _, Route, Server};
use std::{collections::HashMap, sync::Arc};

use async_std::sync::RwLock;
use futures::{AsyncRead, AsyncWrite};
use metrics::{decrement_gauge, increment_counter, increment_gauge};

use crate::utils::home_id_from_domain;

pub const METRICS_AGENT_COUNT: &str = "agent.count";
pub const METRICS_AGENT_LIVE: &str = "agent.live";
pub const METRICS_PROXY_COUNT: &str = "proxy.count";
pub const METRICS_CLUSTER_LIVE: &str = "cluster.live";
pub const METRICS_CLUSTER_COUNT: &str = "cluster.count";
pub const METRICS_PROXY_LIVE: &str = "proxy.live";

mod agent_listener;
mod agent_worker;
mod proxy_listener;
mod tunnel;
mod utils;

pub use agent_listener::quic::{AgentQuicConnection, AgentQuicListener, AgentQuicSubConnection};
pub use agent_listener::tcp::{AgentTcpConnection, AgentTcpListener, AgentTcpSubConnection};
pub use agent_listener::{AgentConnection, AgentListener, AgentSubConnection};

pub use proxy_listener::cluster::{run_sdn, ProxyClusterListener, ProxyClusterTunnel};
pub use proxy_listener::http::{ProxyHttpListener, ProxyHttpTunnel};
pub use proxy_listener::{ProxyListener, ProxyTunnel};

pub use tunnel::{tunnel_task, TunnelContext};

pub async fn run_agent_connection<AG, S, R, W>(
    agent_connection: AG,
    agents: Arc<RwLock<HashMap<NodeAliasId, async_std::channel::Sender<Box<dyn ProxyTunnel>>>>>,
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
    let home_id = home_id_from_domain(&domain);
    agents
        .write()
        .await
        .insert(home_id.clone(), proxy_tunnel_tx);
    node_alias_sdk.register(home_id.clone());
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
        agents.write().await.remove(&home_id);
        node_alias_sdk.unregister(home_id_from_domain(&domain));
        log::info!("agent_worker exit for domain: {}", domain);
        decrement_gauge!(METRICS_AGENT_LIVE, 1.0);
    });
}
