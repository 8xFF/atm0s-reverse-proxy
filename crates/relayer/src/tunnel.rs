use std::{collections::HashMap, error::Error, sync::Arc, time::Duration};

use crate::{
    proxy_listener::ProxyTunnel, utils::domain_hash, METRICS_CLUSTER_COUNT, METRICS_CLUSTER_LIVE,
    METRICS_PROXY_COUNT,
};
use async_std::{prelude::FutureExt, sync::RwLock};
use atm0s_sdn::{
    virtual_socket::{make_insecure_quinn_client, vnet_addr, VirtualNet},
    NodeAliasResult, NodeAliasSdk,
};
use futures::{select, FutureExt as _};
use metrics::{decrement_gauge, increment_counter, increment_gauge};
use protocol::cluster::{ClusterTunnelRequest, ClusterTunnelResponse};

pub enum TunnelContext {
    Cluster,
    Local(NodeAliasSdk, VirtualNet),
}

pub async fn tunnel_task(
    mut proxy_tunnel: Box<dyn ProxyTunnel>,
    agents: Arc<RwLock<HashMap<String, async_std::channel::Sender<Box<dyn ProxyTunnel>>>>>,
    context: TunnelContext,
) {
    match proxy_tunnel.wait().timeout(Duration::from_secs(5)).await {
        Err(_) => {
            log::error!("proxy_tunnel.wait() for checking url timeout");
            return;
        }
        Ok(None) => {
            log::error!("proxy_tunnel.wait() for checking url invalid");
            return;
        }
        _ => {}
    }
    increment_counter!(METRICS_PROXY_COUNT);
    log::info!("proxy_tunnel.domain(): {}", proxy_tunnel.domain());
    let domain = proxy_tunnel.domain().to_string();
    if let Some(agent_tx) = agents.read().await.get(&domain) {
        agent_tx.send(proxy_tunnel).await.ok();
    } else if let TunnelContext::Local(node_alias_sdk, virtual_net) = context {
        if let Err(e) = tunnel_over_cluster(domain, proxy_tunnel, node_alias_sdk, virtual_net).await
        {
            log::error!("tunnel_over_cluster error: {}", e);
        }
    }
}

async fn tunnel_over_cluster(
    domain: String,
    mut proxy_tunnel: Box<dyn ProxyTunnel>,
    node_alias_sdk: NodeAliasSdk,
    virtual_net: VirtualNet,
) -> Result<(), Box<dyn Error>> {
    log::warn!(
        "agent not found for domain: {} in local => finding in cluster",
        domain
    );
    let node_alias_id = domain_hash(&domain);
    let (tx, rx) = async_std::channel::bounded(1);
    node_alias_sdk.find_alias(
        node_alias_id,
        Box::new(move |res| {
            tx.try_send(res).expect("Should send result");
        }),
    );

    let dest = match rx.recv().await? {
        Ok(NodeAliasResult::FromLocal) => {
            log::warn!(
                "something wrong, found alias {} in local but mapper not found",
                domain
            );
            return Err("INTERNAL_ERROR".into());
        }
        Ok(NodeAliasResult::FromHint(dest)) => dest,
        Ok(NodeAliasResult::FromScan(dest)) => dest,
        Err(_e) => return Err("NODE_ALIAS_NOT_FOUND".into()),
    };

    let socket = virtual_net
        .create_udp_socket(0, 20)
        .map_err(|e| format!("{:?}", e))?;
    let client = make_insecure_quinn_client(socket)?;
    let connecting = client.connect(vnet_addr(dest, 80), "localhost")?;
    let connection = connecting.await?;
    let (mut send, mut recv) = connection.open_bi().await?;

    let req_buf: Vec<u8> = (&ClusterTunnelRequest {
        domain: domain.clone(),
    })
        .into();
    send.write_all(&req_buf).await?;
    let mut res_buf = [0; 1500];
    let res_size = recv
        .read(&mut res_buf)
        .await?
        .ok_or("ClusterTunnelResponse not received".to_string())?;
    let res = ClusterTunnelResponse::try_from(&res_buf[..res_size])?;
    if !res.success {
        log::error!("ClusterTunnelResponse not success");
        return Err("ClusterTunnelResponse not success".into());
    }

    log::info!("start cluster proxy tunnel for domain {}", domain);
    increment_counter!(METRICS_CLUSTER_COUNT);
    increment_gauge!(METRICS_CLUSTER_LIVE, 1.0);

    let (mut reader1, mut writer1) = proxy_tunnel.split();
    let job1 = futures::io::copy(&mut reader1, &mut send);
    let job2 = futures::io::copy(&mut recv, &mut writer1);

    select! {
        e = job1.fuse() => {
            if let Err(e) = e {
                log::info!("agent => proxy error: {}", e);
            }
        }
        e = job2.fuse() => {
            if let Err(e) = e {
                log::info!("proxy => agent error: {}", e);
            }
        }
    }

    log::info!("end cluster proxy tunnel for domain {}", domain);
    decrement_gauge!(METRICS_CLUSTER_LIVE, 1.0);
    Ok(())
}
