use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use atm0s_reverse_proxy_agent::{
    run_tunnel_connection, Connection, Protocol, QuicConnection, SubConnection, TcpConnection,
};
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use clap::Parser;
use futures::{AsyncRead, AsyncWrite};
use protocol_ed25519::AgentLocalKey;
use rustls::pki_types::CertificateDer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;

/// A benchmark util for simulating multiple clients connect to relay server
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address of relay server
    #[arg(env, long)]
    connector_addr: Url,

    /// Protocol of relay server
    #[arg(env, long)]
    connector_protocol: Protocol,

    /// Http proxy dest
    #[arg(env, long, default_value = "127.0.0.1:8080")]
    http_dest: SocketAddr,

    /// Sni-https proxy dest
    #[arg(env, long, default_value = "127.0.0.1:8443")]
    https_dest: SocketAddr,

    /// Custom quic server cert in base64
    #[arg(env, long)]
    custom_quic_cert_base64: Option<String>,

    /// Allow connect in insecure mode
    #[arg(env, long)]
    allow_quic_insecure: bool,

    /// clients
    #[arg(env, long)]
    clients: usize,

    /// wait time between connect action
    #[arg(env, long, default_value_t = 1000)]
    connect_wait_ms: u64,
}

#[async_std::main]
async fn main() {
    let args = Args::parse();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("should install ring as default");

    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "warn");
    }
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    for client in 0..args.clients {
        let args2 = args.clone();
        async_std::task::spawn(async move {
            async_std::task::spawn_local(connect(client, args2));
        });
        async_std::task::sleep(Duration::from_millis(args.connect_wait_ms)).await;
    }

    loop {
        async_std::task::sleep(Duration::from_millis(1000)).await;
    }
}

async fn connect(client: usize, args: Args) {
    let default_tunnel_cert_buf = include_bytes!("../../../certs/tunnel.cert");
    let default_tunnel_cert = CertificateDer::from(default_tunnel_cert_buf.to_vec());

    let server_certs = if let Some(cert) = args.custom_quic_cert_base64 {
        vec![CertificateDer::from(
            URL_SAFE
                .decode(&cert)
                .expect("Custom cert should in base64 format")
                .to_vec(),
        )]
    } else {
        vec![default_tunnel_cert]
    };
    let agent_signer = AgentLocalKey::random();

    loop {
        log::info!(
            "Connecting to connector... {:?} addr: {}",
            args.connector_protocol,
            args.connector_addr
        );
        let started = Instant::now();
        match args.connector_protocol {
            Protocol::Tcp => {
                match TcpConnection::new(args.connector_addr.clone(), &agent_signer).await {
                    Ok(conn) => {
                        log::info!(
                            "Connected to connector via tcp with res {:?}",
                            conn.response()
                        );
                        println!("{client} connected after {:?}", started.elapsed());
                        run_connection_loop(conn, args.http_dest, args.https_dest).await;
                    }
                    Err(e) => {
                        log::error!("Connect to connector via tcp error: {}", e);
                    }
                }
            }
            Protocol::Quic => {
                match QuicConnection::new(
                    args.connector_addr.clone(),
                    &agent_signer,
                    &server_certs,
                    args.allow_quic_insecure,
                )
                .await
                {
                    Ok(conn) => {
                        log::info!(
                            "Connected to connector via quic with res {:?}",
                            conn.response()
                        );
                        println!("{client} connected after {:?}", started.elapsed());
                        run_connection_loop(conn, args.http_dest, args.https_dest).await;
                    }
                    Err(e) => {
                        log::error!("Connect to connector via quic error: {}", e);
                    }
                }
            }
        }
        //TODO exponential backoff
        async_std::task::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn run_connection_loop<S, R, W>(
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
