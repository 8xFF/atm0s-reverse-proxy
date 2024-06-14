use std::{
    error::Error,
    net::{SocketAddr, SocketAddrV4},
    time::Duration,
};

use crate::{
    proxy_listener::{
        cluster::{make_quinn_client, AliasSdk, VirtualUdpSocket},
        ProxyTunnel,
    },
    utils::home_id_from_domain,
    AgentStore, METRICS_CLUSTER_COUNT, METRICS_CLUSTER_LIVE, METRICS_PROXY_COUNT,
};
use async_std::prelude::FutureExt;
use futures::{select, FutureExt as _};
use metrics::{counter, gauge};
use protocol::cluster::{ClusterTunnelRequest, ClusterTunnelResponse};
use rustls::pki_types::CertificateDer;

pub enum TunnelContext<'a> {
    Cluster,
    Local(AliasSdk, VirtualUdpSocket, Vec<CertificateDer<'a>>),
}

pub async fn tunnel_task<'a>(
    mut proxy_tunnel: Box<dyn ProxyTunnel>,
    agents: AgentStore,
    context: TunnelContext<'a>,
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
    counter!(METRICS_PROXY_COUNT).increment(1);
    log::info!("proxy_tunnel.domain(): {}", proxy_tunnel.domain());
    let domain = proxy_tunnel.domain().to_string();
    let home_id = home_id_from_domain(&domain);
    if let Some(agent_tx) = agents.get(home_id) {
        agent_tx.send(proxy_tunnel).await.ok();
    } else if let TunnelContext::Local(node_alias_sdk, virtual_net, server_certs) = context {
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
        }
    }
}

async fn tunnel_over_cluster<'a>(
    domain: String,
    mut proxy_tunnel: Box<dyn ProxyTunnel>,
    node_alias_sdk: AliasSdk,
    socket: VirtualUdpSocket,
    server_certs: &'a [CertificateDer<'a>],
) -> Result<(), Box<dyn Error>> {
    log::warn!(
        "agent not found for domain: {} in local => finding in cluster",
        domain
    );
    let node_alias_id = home_id_from_domain(&domain);
    let dest = node_alias_sdk
        .find_alias(node_alias_id)
        .await
        .ok_or("NODE_ALIAS_NOT_FOUND".to_string())?;
    log::info!("found agent for domain: {domain} in node {dest}");
    let client = make_quinn_client(socket, server_certs)?;
    log::info!("connecting to agent for domain: {domain} in node {dest}");
    let connecting = client.connect(
        SocketAddr::V4(SocketAddrV4::new(dest.into(), 443)),
        "cluster",
    )?;
    let connection = connecting.await?;
    log::info!("connected to agent for domain: {domain} in node {dest}");
    let (mut send, mut recv) = connection.open_bi().await?;
    log::info!("opened bi stream to agent for domain: {domain} in node {dest}");

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
    counter!(METRICS_CLUSTER_COUNT).increment(1);
    gauge!(METRICS_CLUSTER_LIVE).increment(1.0);

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
    gauge!(METRICS_CLUSTER_LIVE).decrement(1.0);
    Ok(())
}
