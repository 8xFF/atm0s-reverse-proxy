use std::sync::Arc;

use async_std::io::WriteExt;

use futures::{select, AsyncRead, AsyncReadExt, AsyncWrite, FutureExt};
use local_tunnel::tcp::LocalTcpTunnel;
use protocol::cluster::AgentTunnelRequest;

use crate::local_tunnel::LocalTunnel;

mod connection;
mod local_tunnel;

pub use connection::{
    quic::{QuicConnection, QuicSubConnection},
    tcp::{TcpConnection, TcpSubConnection},
    Connection, Protocol, SubConnection,
};
pub use local_tunnel::{registry::SimpleServiceRegistry, ServiceRegistry};

pub async fn run_tunnel_connection<S, R, W>(sub_connection: S, registry: Arc<dyn ServiceRegistry>)
where
    S: SubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin,
{
    log::info!("sub_connection pipe to local_tunnel start");
    let (mut reader1, mut writer1) = sub_connection.split();
    let mut first_pkt = [0u8; 4096];
    let (local_tunnel, first_pkt_start, first_pkt_end) = match reader1.read(&mut first_pkt).await {
        Ok(first_pkt_len) => {
            log::info!("first pkt size: {}", first_pkt_len);
            if first_pkt_len < 2 {
                log::error!("first pkt size is < 4 => close");
                return;
            }
            let handshake_len = u16::from_be_bytes([first_pkt[0], first_pkt[1]]) as usize;
            if handshake_len + 2 > first_pkt_len {
                log::error!("first pkt size is < handshake {handshake_len} + 2 => close");
                return;
            }
            match AgentTunnelRequest::try_from(&first_pkt[2..(handshake_len + 2)]) {
                Ok(handshake) => {
                    if let Some(dest) =
                        registry.dest_for(handshake.tls, handshake.service, &handshake.domain)
                    {
                        log::info!("create tunnel to dest {}", dest);
                        (
                            LocalTcpTunnel::new(dest).await,
                            handshake_len + 2,
                            first_pkt_len,
                        )
                    } else {
                        log::warn!(
                            "dest for service {:?} tls {} domain {} not found",
                            handshake.service,
                            handshake.tls,
                            handshake.domain
                        );
                        return;
                    }
                }
                Err(e) => {
                    log::error!("handshake parse error: {}", e);
                    return;
                }
            }
        }
        Err(e) => {
            log::error!("read first pkt error: {}", e);
            return;
        }
    };

    let local_tunnel = match local_tunnel {
        Ok(local_tunnel) => local_tunnel,
        Err(e) => {
            log::error!("create local_tunnel error: {}", e);
            return;
        }
    };

    let (mut reader2, mut writer2) = local_tunnel.split();

    if let Err(e) = writer2
        .write_all(&first_pkt[first_pkt_start..first_pkt_end])
        .await
    {
        log::error!("write first pkt to local_tunnel error: {}", e);
        return;
    }

    let job1 = futures::io::copy(&mut reader1, &mut writer2);
    let job2 = futures::io::copy(&mut reader2, &mut writer1);

    select! {
        e = job1.fuse() => {
            if let Err(e) = e {
                log::error!("job1 error: {}", e);
            }
        }
        e = job2.fuse() => {
            if let Err(e) = e {
                log::error!("job2 error: {}", e);
            }
        }
    }
    log::info!("sub_connection pipe to local_tunnel stop");
}
