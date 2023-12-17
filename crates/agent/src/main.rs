use std::alloc::System;

#[global_allocator]
static A: System = System;

use std::net::SocketAddr;

use async_std::io::WriteExt;
use clap::Parser;

use futures::{select, AsyncReadExt, FutureExt};
use local_tunnel::tcp::LocalTcpTunnel;
use protocol::key::LocalKey;
use tracing_subscriber::{fmt, layer::*, util::SubscriberInitExt, EnvFilter};

use crate::{
    connection::{tcp::TcpConnection, Connection, SubConnection},
    local_tunnel::LocalTunnel,
};

mod connection;
mod local_tunnel;

/// A HTTP and SNI HTTPs proxy for expose your local service to the internet.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of times to greet
    #[arg(env, long, long, default_value = "127.0.0.1:33333")]
    tcp_connector_addr: SocketAddr,

    /// Http proxy dest
    #[arg(env, long, default_value = "127.0.0.1:8080")]
    http_dest: SocketAddr,

    /// Sni-https proxy dest
    #[arg(env, long, default_value = "127.0.0.1:8443")]
    https_dest: SocketAddr,

    /// Persistent local key
    #[arg(env, long, default_value = "local_key.pem")]
    local_key: String,
}

#[async_std::main]
async fn main() {
    let args = Args::parse();

    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    //read local_key from file first, if not exist, create a new one and save to file
    let local_key = match std::fs::read_to_string(&args.local_key) {
        Ok(local_key) => match LocalKey::from_pem(&local_key) {
            Some(local_key) => {
                log::info!("loadded local_key: \n{}", local_key.to_pem());
                local_key
            }
            None => {
                log::error!("read local_key from file error: invalid pem");
                return;
            }
        },
        Err(e) => {
            //check if file not exist
            if e.kind() != std::io::ErrorKind::NotFound {
                log::error!("read local_key from file error: {}", e);
                return;
            }

            log::warn!("local_key file not found => regenerate");
            let local_key = LocalKey::random();
            log::info!("created local_key: \n{}", local_key.to_pem());
            if let Err(e) = std::fs::write(&args.local_key, local_key.to_pem()) {
                log::error!("write local_key to file error: {}", e);
                return;
            }
            local_key
        }
    };

    loop {
        log::info!("Connecting to connector...");
        if let Some(mut connection) = TcpConnection::new(args.tcp_connector_addr, &local_key).await
        {
            log::info!("Connection to connector is established");
            while let Some(sub_connection) = connection.recv().await {
                let http_dest = args.http_dest;
                let https_dest = args.https_dest;
                async_std::task::spawn_local(async move {
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
                                (LocalTcpTunnel::new(https_dest).await, first_pkt_len)
                            } else {
                                (LocalTcpTunnel::new(http_dest).await, first_pkt_len)
                            }
                        }
                        Err(e) => {
                            log::error!("read first pkt error: {}", e);
                            return;
                        }
                    };

                    if let Some(local_tunnel) = local_tunnel {
                        let (mut reader2, mut writer2) = local_tunnel.split();

                        if let Err(e) = writer2.write_all(&first_pkt[..first_pkt_len]).await {
                            log::error!("write first pkt to local_tunnel error: {}", e);
                            return;
                        }

                        let job1 = futures::io::copy(&mut reader1, &mut writer2);
                        let job2 = futures::io::copy(&mut reader2, &mut writer1);

                        select! {
                            _ = job1.fuse() => {}
                            _ = job2.fuse() => {}
                        }
                    }
                    log::info!("sub_connection pipe to local_tunnel stop");
                });
            }
            log::warn!("Connection to connector is closed, try to reconnect...");
        }
        //TODO exponential backoff
        async_std::task::sleep(std::time::Duration::from_secs(1)).await;
    }
}
