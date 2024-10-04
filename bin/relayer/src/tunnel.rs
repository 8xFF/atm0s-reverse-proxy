use std::time::Instant;

use crate::{
    proxy_listener::{
        cluster::{AliasSdk, ProxyClusterOutgoingTunnel, VirtualUdpSocket},
        ProxyTunnel, ProxyTunnelWrap,
    },
    utils::home_id_from_domain,
    AgentStore, METRICS_PROXY_CLUSTER_COUNT, METRICS_PROXY_CLUSTER_ERROR_COUNT,
    METRICS_PROXY_HTTP_COUNT, METRICS_PROXY_HTTP_ERROR_COUNT, METRICS_PROXY_HTTP_LIVE,
    METRICS_TUNNEL_CLUSTER_COUNT, METRICS_TUNNEL_CLUSTER_ERROR_COUNT,
    METRICS_TUNNEL_CLUSTER_HISTOGRAM, METRICS_TUNNEL_CLUSTER_LIVE,
};
use anyhow::anyhow;
use metrics::{counter, gauge, histogram};
use protocol::cluster::{wait_object, write_object, ClusterTunnelRequest, ClusterTunnelResponse};
use rustls::pki_types::CertificateDer;
use tokio::io::copy_bidirectional;

pub enum TunnelContext<'a> {
    Cluster,
    Local(AliasSdk, VirtualUdpSocket, Vec<CertificateDer<'a>>),
}

impl<'a> TunnelContext<'a> {
    fn is_local(&self) -> bool {
        matches!(self, Self::Local(..))
    }
}

pub async fn tunnel_task<P: ProxyTunnel + Into<ProxyTunnelWrap>>(
    mut proxy_tunnel: P,
    agents: AgentStore,
    context: TunnelContext<'_>,
) {
    if context.is_local() {
        counter!(METRICS_PROXY_HTTP_COUNT).increment(1);
    } else {
        counter!(METRICS_PROXY_CLUSTER_COUNT).increment(1);
    }
    log::info!(
        "proxy_tunnel.wait() for checking url from {}",
        proxy_tunnel.source_addr()
    );
    if proxy_tunnel.wait().await.is_none() {
        log::error!("proxy_tunnel.wait() for checking url invalid");
        if context.is_local() {
            counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
        } else {
            counter!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1);
        }
        return;
    }

    log::info!(
        "proxy_tunnel.wait() done and got domain {}",
        proxy_tunnel.domain()
    );
    let domain = proxy_tunnel.domain().to_string();
    let home_id = home_id_from_domain(&domain);
    if let Some(agent_tx) = agents.get(home_id) {
        agent_tx.send(proxy_tunnel.into()).await.ok();
    } else if let TunnelContext::Local(node_alias_sdk, virtual_net, server_certs) = context {
        counter!(METRICS_TUNNEL_CLUSTER_COUNT).increment(1);
        gauge!(METRICS_TUNNEL_CLUSTER_LIVE).increment(1.0);
        gauge!(METRICS_PROXY_HTTP_LIVE).increment(1.0);
        if let Err(e) = tunnel_over_cluster(
            domain,
            proxy_tunnel.into(),
            node_alias_sdk,
            virtual_net,
            &server_certs,
        )
        .await
        {
            log::error!("tunnel_over_cluster error: {}", e);
            counter!(METRICS_TUNNEL_CLUSTER_ERROR_COUNT).increment(1);
        }
        gauge!(METRICS_TUNNEL_CLUSTER_LIVE).decrement(1.0);
        gauge!(METRICS_PROXY_HTTP_LIVE).decrement(1.0);
    } else {
        log::warn!("dont support tunnel over 2 relayed, it should be single e2e quic connection");
        counter!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1);
    }
}

async fn tunnel_over_cluster<'a>(
    domain: String,
    mut proxy_tunnel: ProxyTunnelWrap,
    node_alias_sdk: AliasSdk,
    socket: VirtualUdpSocket,
    server_certs: &'a [CertificateDer<'a>],
) -> anyhow::Result<()> {
    let started = Instant::now();
    log::warn!(
        "[TunnerOverCluster] agent not found for domain: {domain} in local => finding in cluster",
    );
    let node_alias_id = home_id_from_domain(&domain);
    let dest = node_alias_sdk
        .find_alias(node_alias_id)
        .await
        .ok_or(anyhow!("NODE_ALIAS_NOT_FOUND"))?;
    log::info!("[TunnerOverCluster]  found agent for domain: {domain} in node {dest}");

    let mut outgoing_tunnel =
        ProxyClusterOutgoingTunnel::connect(socket, dest.into(), &domain, server_certs).await?;

    histogram!(METRICS_TUNNEL_CLUSTER_HISTOGRAM)
        .record(started.elapsed().as_millis() as f32 / 1000.0);

    let req = ClusterTunnelRequest {
        domain: domain.clone(),
        handshake: proxy_tunnel.handshake().to_vec(),
    };

    write_object::<_, _, 1000>(&mut outgoing_tunnel, req).await?;
    let res = wait_object::<_, ClusterTunnelResponse, 1000>(&mut outgoing_tunnel).await?;
    if !res.success {
        log::error!("[TunnerOverCluster] ClusterTunnelResponse not success");
        return Err(anyhow!("ClusterTunnelResponse not success"));
    }

    log::info!("[TunnerOverCluster] start cluster proxy tunnel for domain {domain}");

    match copy_bidirectional(&mut proxy_tunnel, &mut outgoing_tunnel).await {
        Ok(res) => {
            log::info!(
                "[TunnerOverCluster] end cluster proxy tunnel for domain {domain}, res {res:?}"
            );
        }
        Err(e) => {
            log::error!(
                "[TunnerOverCluster] end cluster proxy tunnel for domain {domain} err {e:?}"
            );
        }
    }

    Ok(())
}
