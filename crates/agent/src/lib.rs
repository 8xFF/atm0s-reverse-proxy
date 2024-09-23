use std::sync::Arc;

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

    let local_tunnel = match wait_handshake(&mut reader1).await {
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

pub async fn wait_handshake<R: AsyncRead + Send + Unpin>(
    reader: &mut R,
) -> Result<AgentTunnelRequest, String> {
    let mut len_buf = [0; 2];
    let mut data_buf = [0; 1000];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| e.to_string())?;
    let handshake_len = u16::from_be_bytes([len_buf[0], len_buf[1]]) as usize;
    log::info!("first pkt size: {}", handshake_len);
    if handshake_len > data_buf.len() {
        return Err("Handshake package too big".to_string());
    }

    reader
        .read_exact(&mut data_buf[0..handshake_len])
        .await
        .map_err(|e| e.to_string())?;

    log::info!("got first pkt with size: {}", handshake_len);

    AgentTunnelRequest::try_from(&data_buf[0..handshake_len]).map_err(|e| e.to_string())
}
