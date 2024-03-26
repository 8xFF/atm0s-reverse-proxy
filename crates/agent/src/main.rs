use std::{alloc::System, net::SocketAddr};

use atm0s_reverse_proxy_agent::{
    run_tunnel_connection, Connection, Protocol, QuicConnection, SubConnection, TcpConnection,
};
use clap::Parser;
use futures::{AsyncRead, AsyncWrite};
use protocol_ed25519::AgentLocalKey;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[global_allocator]
static A: System = System;

/// A HTTP and SNI HTTPs proxy for expose your local service to the internet.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address of relay server
    #[arg(env, long)]
    connector_addr: SocketAddr,

    /// Protocol of relay server
    #[arg(env, long)]
    connector_protocol: Protocol,

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
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("should install ring as default");
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
    let agent_signer = match std::fs::read_to_string(&args.local_key) {
        Ok(local_key) => match AgentLocalKey::from_pem(&local_key) {
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
            let local_key = AgentLocalKey::random();
            log::info!("created local_key: \n{}", local_key.to_pem());
            if let Err(e) = std::fs::write(&args.local_key, local_key.to_pem()) {
                log::error!("write local_key to file error: {}", e);
                return;
            }
            local_key
        }
    };

    loop {
        log::info!(
            "Connecting to connector... {:?} addr: {:?}",
            args.connector_protocol,
            args.connector_addr
        );
        match args.connector_protocol {
            Protocol::Tcp => match TcpConnection::new(args.connector_addr, &agent_signer).await {
                Ok(conn) => {
                    log::info!("Connected to connector via tcp");
                    run_connection_loop(conn, args.http_dest, args.https_dest).await;
                }
                Err(e) => {
                    log::error!("Connect to connector via tcp error: {}", e);
                }
            },
            Protocol::Quic => match QuicConnection::new(args.connector_addr, &agent_signer).await {
                Ok(conn) => {
                    log::info!("Connected to connector via quic");
                    run_connection_loop(conn, args.http_dest, args.https_dest).await;
                }
                Err(e) => {
                    log::error!("Connect to connector via quic error: {}", e);
                }
            },
        }
        //TODO exponential backoff
        async_std::task::sleep(std::time::Duration::from_secs(1)).await;
    }
}

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
                async_std::task::spawn_local(run_tunnel_connection(
                    sub_connection,
                    http_dest,
                    https_dest,
                ));
            }
            Err(e) => {
                log::error!("recv sub_connection error: {}", e);
                break;
            }
        }
    }
}
