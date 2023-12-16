use futures::{select, FutureExt};
use tracing_subscriber::{layer::*, fmt, EnvFilter, util::SubscriberInitExt};
use connection::tcp::TcpConnection;
use local_tunnel::tcp::LocalTcpTunnel;

use crate::{connection::{Connection, SubConnection}, local_tunnel::LocalTunnel};

mod connection;
mod local_tunnel;

#[async_std::main]
async fn main() {
    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();
    loop {
        log::info!("Connecting to connector...");
        if let Some(mut connection) = TcpConnection::new("127.0.0.1:3333", "localhost:8080").await {
            log::info!("Connection to connector is established");
            while let Some(sub_connection) = connection.recv().await {
                async_std::task::spawn_local(async move {
                    log::info!("sub_connection pipe to local_tunnel start");
                    if let Some(local_tunnel) = LocalTcpTunnel::new("127.0.0.1:8081".parse().expect("")).await {
                        let (mut reader1, mut writer1) = sub_connection.split();
                        let (mut reader2, mut writer2) = local_tunnel.split();

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
