use std::{
    error::Error,
    net::{SocketAddr, SocketAddrV4},
    time::{Duration, Instant},
};

use crate::{
    proxy_listener::{
        cluster::{make_quinn_client, AliasSdk, VirtualUdpSocket},
        ProxyTunnel,
    },
    utils::home_id_from_domain,
    AgentStore, METRICS_PROXY_CLUSTER_COUNT, METRICS_PROXY_CLUSTER_ERROR_COUNT,
    METRICS_PROXY_HTTP_COUNT, METRICS_PROXY_HTTP_ERROR_COUNT, METRICS_PROXY_HTTP_LIVE,
    METRICS_TUNNEL_CLUSTER_COUNT, METRICS_TUNNEL_CLUSTER_ERROR_COUNT,
    METRICS_TUNNEL_CLUSTER_HISTOGRAM, METRICS_TUNNEL_CLUSTER_LIVE,
};
use async_std::prelude::FutureExt;
use futures::{select, FutureExt as _};
use metrics::{counter, gauge, histogram};
use protocol::cluster::{ClusterTunnelRequest, ClusterTunnelResponse};
use rustls::pki_types::CertificateDer;

pub enum TunnelContext<'a> {
    Cluster,
    Local(AliasSdk, VirtualUdpSocket, Vec<CertificateDer<'a>>),
}

impl<'a> TunnelContext<'a> {
    fn is_local(&self) -> bool {
        matches!(self, Self::Local(..))
    }
}

pub async fn tunnel_task(
    mut proxy_tunnel: Box<dyn ProxyTunnel>,
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
    match proxy_tunnel.wait().timeout(Duration::from_secs(5)).await {
        Err(_) => {
            log::error!("proxy_tunnel.wait() for checking url timeout");
            if context.is_local() {
                counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
            } else {
                counter!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1);
            }
            return;
        }
        Ok(None) => {
            log::error!("proxy_tunnel.wait() for checking url invalid");
            if context.is_local() {
                counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
            } else {
                counter!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1);
            }
            return;
        }
        _ => {}
    }

    log::info!(
        "proxy_tunnel.wait() done and got domain {}",
        proxy_tunnel.domain()
    );
    let domain = proxy_tunnel.domain().to_string();
    let home_id = home_id_from_domain(&domain);
    if let Some(agent_tx) = agents.get(home_id) {
        agent_tx.send(proxy_tunnel).await.ok();
    } else if let TunnelContext::Local(node_alias_sdk, virtual_net, server_certs) = context {
        counter!(METRICS_TUNNEL_CLUSTER_COUNT).increment(1);
        gauge!(METRICS_TUNNEL_CLUSTER_LIVE).increment(1.0);
        gauge!(METRICS_PROXY_HTTP_LIVE).increment(1.0);
        if let Err(e) = tunnel_over_cluster(
            domain,
            proxy_tunnel,
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
    mut proxy_tunnel: Box<dyn ProxyTunnel>,
    node_alias_sdk: AliasSdk,
    socket: VirtualUdpSocket,
    server_certs: &'a [CertificateDer<'a>],
) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    log::warn!(
        "[TunnerOverCluster] agent not found for domain: {} in local => finding in cluster",
        domain
    );
    let node_alias_id = home_id_from_domain(&domain);
    let dest = node_alias_sdk
        .find_alias(node_alias_id)
        .await
        .ok_or("NODE_ALIAS_NOT_FOUND".to_string())?;
    log::info!("[TunnerOverCluster]  found agent for domain: {domain} in node {dest}");
    let client = make_quinn_client(socket, server_certs)?;
    log::info!("[TunnerOverCluster] connecting to agent for domain: {domain} in node {dest}");
    let connecting = client.connect(
        SocketAddr::V4(SocketAddrV4::new(dest.into(), 443)),
        "cluster",
    )?;
    let connection = connecting.await?;
    log::info!("[TunnerOverCluster]  connected to agent for domain: {domain} in node {dest}");
    let (mut send, mut recv) = connection.open_bi().await?;
    log::info!("[TunnerOverCluster] opened bi stream to agent for domain: {domain} in node {dest}");

    histogram!(METRICS_TUNNEL_CLUSTER_HISTOGRAM)
        .record(started.elapsed().as_millis() as f32 / 1000.0);

    let req_buf: Vec<u8> = (&ClusterTunnelRequest {
        domain: domain.clone(),
        handshake: proxy_tunnel.handshake().to_vec(),
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
    Ok(())
}
