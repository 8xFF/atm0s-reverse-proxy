use std::sync::Arc;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use local_tunnel::tcp::LocalTcpTunnel;
use protocol::cluster::{wait_object, AgentTunnelRequest};

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

    let local_tunnel = match wait_object::<_, AgentTunnelRequest, 1000>(&mut reader1).await {
        Ok(handshake) => {
            log::info!(
                "sub_connection pipe with handshake: tls: {}, {}/{:?} ",
                handshake.tls,
                handshake.domain,
                handshake.service
            );
            if let Some(dest) =
                registry.dest_for(handshake.tls, handshake.service, &handshake.domain)
            {
                log::info!("create tunnel to dest {}", dest);
                LocalTcpTunnel::new(dest).await
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
    let job1 = async {
        log::info!("task1 start");
        let res = futures::io::copy(&mut reader1, &mut writer2).await;
        writer2.flush().await;
        writer2.close().await;
        log::info!("task1 end {res:?}");
        res
    };
    let job2 = async {
        log::info!("task2 start");
        let res = futures::io::copy(&mut reader2, &mut writer1).await;
        writer1.flush().await;
        writer1.close().await;
        log::info!("task2 end {res:?}");
        res
    };

    let (res1, res2) = futures::join!(job1, job2);

    match res1 {
        Ok(len) => log::info!("proxy from server to local end with {} bytes", len),
        Err(err) => log::error!("proxy from server to local error {err:?}"),
    }

    match res2 {
        Ok(len) => log::info!("proxy from local to server end with {} bytes", len),
        Err(err) => log::error!("proxy from local to server error {err:?}"),
    }

    log::info!("sub_connection pipe to local_tunnel stop");
}
