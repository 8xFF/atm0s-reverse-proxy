use std::net::SocketAddr;

use async_std::io::WriteExt;

use futures::{select, AsyncRead, AsyncReadExt, AsyncWrite, FutureExt};
use local_tunnel::tcp::LocalTcpTunnel;

use crate::{
    connection::{Connection, SubConnection},
    local_tunnel::LocalTunnel,
};

mod connection;
mod local_tunnel;

pub use connection::{
    quic::{QuicConnection, QuicSubConnection},
    tcp::{TcpConnection, TcpSubConnection},
    Protocol,
};

pub async fn run_connection_loop<S, R, W>(
    mut connection: impl Connection<S, R, W>,
    http_dest: SocketAddr,
    https_dest: SocketAddr,
) where
    S: SubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    loop {
        match connection.recv().await {
            Ok(sub_connection) => {
                log::info!("recv sub_connection");
                async_std::task::spawn_local(run_connection(sub_connection, http_dest, https_dest));
            }
            Err(e) => {
                log::error!("recv sub_connection error: {}", e);
                break;
            }
        }
    }
}

async fn run_connection<S, R, W>(sub_connection: S, http_dest: SocketAddr, https_dest: SocketAddr)
where
    S: SubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin,
{
    log::info!("sub_connection pipe to local_tunnel start");
    let (mut reader1, mut writer1) = sub_connection.split();
    let mut first_pkt = [0u8; 4096];
    let (local_tunnel, first_pkt_len) = match reader1.read(&mut first_pkt).await {
        Ok(first_pkt_len) => {
            log::info!("first pkt size: {}", first_pkt_len);
            if first_pkt_len == 0 {
                log::error!("first pkt size is 0 => close");
                return;
            }
            if first_pkt[0] == 0x16 {
                log::info!("create tunnel to https dest {}", https_dest);
                (LocalTcpTunnel::new(https_dest).await, first_pkt_len)
            } else {
                log::info!("create tunnel to http dest {}", http_dest);
                (LocalTcpTunnel::new(http_dest).await, first_pkt_len)
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

    if let Err(e) = writer2.write_all(&first_pkt[..first_pkt_len]).await {
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
