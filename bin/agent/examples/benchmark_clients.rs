use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use atm0s_reverse_proxy_agent::{
    run_tunnel_connection, Connection, Protocol, QuicConnection, ServiceRegistry,
    SimpleServiceRegistry, SubConnection, TcpConnection,
};
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use clap::Parser;
use protocol::DEFAULT_TUNNEL_CERT;
use protocol_ed25519::AgentLocalKey;
use rustls::pki_types::CertificateDer;
use tokio::time::sleep;
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

#[tokio::main]
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

    let registry = SimpleServiceRegistry::new(args.http_dest, args.https_dest);
    let registry = Arc::new(registry);

    for client in 0..args.clients {
        let args_c = args.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            connect(client, args_c, registry).await;
        });
        sleep(Duration::from_millis(args.connect_wait_ms)).await;
    }

    loop {
        sleep(Duration::from_millis(1000)).await;
    }
}

async fn connect(client: usize, args: Args, registry: Arc<dyn ServiceRegistry>) {
    let default_tunnel_cert = CertificateDer::from(DEFAULT_TUNNEL_CERT.to_vec());

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
                        run_connection_loop(conn, registry.clone()).await;
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
                        run_connection_loop(conn, registry.clone()).await;
                    }
                    Err(e) => {
                        log::error!("Connect to connector via quic error: {}", e);
                    }
                }
            }
        }
        //TODO exponential backoff
        sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn run_connection_loop<S>(
    mut connection: impl Connection<S>,
    registry: Arc<dyn ServiceRegistry>,
) where
    S: SubConnection + 'static,
{
    loop {
        match connection.recv().await {
            Ok(sub_connection) => {
                log::info!("recv sub_connection");
                let registry = registry.clone();
                tokio::spawn(run_tunnel_connection(sub_connection, registry));
            }
            Err(e) => {
                log::error!("recv sub_connection error: {}", e);
                break;
            }
        }
    }
}
