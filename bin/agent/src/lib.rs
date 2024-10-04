use std::sync::Arc;

use protocol::cluster::{wait_object, AgentTunnelRequest};

mod connection;
mod local_tunnel;

pub use connection::{
    quic::{QuicConnection, QuicSubConnection},
    tcp::{TcpConnection, TcpSubConnection},
    Connection, Protocol, SubConnection,
};
pub use local_tunnel::{registry::SimpleServiceRegistry, LocalTunnel, ServiceRegistry};
use tokio::{io::copy_bidirectional, net::TcpStream};

pub async fn run_tunnel_connection<S>(
    mut incoming_proxy_conn: S,
    registry: Arc<dyn ServiceRegistry>,
) where
    S: SubConnection,
{
    log::info!("sub_connection pipe to local_tunnel start");

    let local_tunnel =
        match wait_object::<_, AgentTunnelRequest, 1000>(&mut incoming_proxy_conn).await {
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
                    TcpStream::connect(dest).await
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

    let mut local_tunnel = match local_tunnel {
        Ok(local_tunnel) => local_tunnel,
        Err(e) => {
            log::error!("create local_tunnel error: {}", e);
            return;
        }
    };

    log::info!("sub_connection pipe to local_tunnel start");

    match copy_bidirectional(&mut incoming_proxy_conn, &mut local_tunnel).await {
        Ok(res) => {
            log::info!("sub_connection pipe to local_tunnel stop res {res:?}");
        }
        Err(e) => {
            log::error!("sub_connection pipe to local_tunnel stop err {e:?}");
        }
    }
}
