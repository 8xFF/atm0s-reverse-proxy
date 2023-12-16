use std::{collections::HashMap, process::exit, sync::Arc};

use agent_listener::tcp::AgentTcpListener;
use async_std::sync::RwLock;
use futures::{select, FutureExt};
use proxy_listener::http::ProxyHttpListener;
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt, EnvFilter};

use crate::{
    agent_listener::{AgentConnection, AgentListener},
    proxy_listener::{ProxyListener, ProxyTunnel},
};

mod agent_listener;
mod agent_worker;
mod proxy_listener;

#[async_std::main]
async fn main() {
    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    let mut agent_listener = AgentTcpListener::new(3333)
        .await
        .expect("Should listen agent port");
    let mut proxy_http_listener = ProxyHttpListener::new(8080, false)
        .await
        .expect("Should listen http port");
    let mut proxy_tls_listener = ProxyHttpListener::new(8443, true)
        .await
        .expect("Should listen tls port");
    let agents = Arc::new(RwLock::new(HashMap::new()));

    loop {
        select! {
            e = agent_listener.recv().fuse() => match e {
                Some(agent_connection) => {
                    log::info!("agent_connection.domain(): {}", agent_connection.domain());
                    let domain = agent_connection.domain().to_string();
                    let (mut agent_worker, proxy_tunnel_tx) = agent_worker::AgentWorker::new(agent_connection);
                    agents.write().await.insert(domain.clone(), proxy_tunnel_tx);
                    let agents = agents.clone();
                    async_std::task::spawn_local(async move {
                        log::info!("agent_worker run for domain: {}", domain);
                        while let Some(_) = agent_worker.run().await {}
                        agents.write().await.remove(&domain);
                        log::info!("agent_worker exit for domain: {}", domain);
                    });
                }
                None => {
                    log::error!("agent_listener error");
                    exit(1);
                }
            },
            e = proxy_http_listener.recv().fuse() => match e {
                Some(mut proxy_tunnel) => {
                    if proxy_tunnel.wait().await.is_none() {
                        continue;
                    }
                    log::info!("proxy_tunnel.domain(): {}", proxy_tunnel.domain());
                    let domain = proxy_tunnel.domain().to_string();
                    if let Some(agent_tx) = agents.read().await.get(&domain) {
                        agent_tx.send(proxy_tunnel).await.ok();
                    } else {
                        log::warn!("agent not found for domain: {}", domain);
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_tls_listener.recv().fuse() => match e {
                Some(mut proxy_tunnel) => {
                    if proxy_tunnel.wait().await.is_none() {
                        continue;
                    }
                    log::info!("proxy_tunnel.domain(): {}", proxy_tunnel.domain());
                    let domain = proxy_tunnel.domain().to_string();
                    if let Some(agent_tx) = agents.read().await.get(&domain) {
                        agent_tx.send(proxy_tunnel).await.ok();
                    } else {
                        log::warn!("agent not found for domain: {}", domain);
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
        }
    }
}
