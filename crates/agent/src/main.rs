use std::{alloc::System, net::SocketAddr, sync::Arc};

use atm0s_reverse_proxy_agent::{
    run_tunnel_connection, Connection, Protocol, QuicConnection, ServiceRegistry,
    SimpleServiceRegistry, SubConnection, TcpConnection,
};
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use clap::Parser;
use futures::{AsyncRead, AsyncWrite};
use protocol::services::SERVICE_RTSP;
use protocol_ed25519::AgentLocalKey;
use rustls::pki_types::CertificateDer;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

#[global_allocator]
static A: System = System;

/// A HTTP and SNI HTTPs proxy for expose your local service to the internet.
#[derive(Parser, Debug)]
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

    /// Rtsp proxy dest
    #[arg(env, long, default_value = "127.0.0.1:554")]
    rtsp_dest: SocketAddr,

    /// Sni-https proxy dest
    #[arg(env, long, default_value = "127.0.0.1:5443")]
    rtsps_dest: SocketAddr,

    /// Persistent local key
    #[arg(env, long, default_value = "local_key.pem")]
    local_key: String,

    /// Custom quic server cert in base64
    #[arg(env, long)]
    custom_quic_cert_base64: Option<String>,

    /// Allow connect in insecure mode
    #[arg(env, long)]
    allow_quic_insecure: bool,
}

#[async_std::main]
async fn main() {
    let args = Args::parse();
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

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("should install ring as default");

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

    let mut registry = SimpleServiceRegistry::new(args.http_dest, args.https_dest);
    registry.set_tcp_service(SERVICE_RTSP, args.rtsp_dest);
    registry.set_tls_service(SERVICE_RTSP, args.rtsps_dest);
    let registry = Arc::new(registry);

    loop {
        log::info!(
            "Connecting to connector... {:?} addr: {}",
            args.connector_protocol,
            args.connector_addr
        );
        match args.connector_protocol {
            Protocol::Tcp => {
                match TcpConnection::new(args.connector_addr.clone(), &agent_signer).await {
                    Ok(conn) => {
                        log::info!(
                            "Connected to connector via tcp with res {:?}",
                            conn.response()
                        );
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
                        run_connection_loop(conn, registry.clone()).await;
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

pub async fn run_connection_loop<S, R, W>(
    mut connection: impl Connection<S, R, W>,
    registry: Arc<dyn ServiceRegistry>,
) where
    S: SubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    loop {
        match connection.recv().await {
            Ok(sub_connection) => {
                log::info!("recv sub_connection");
                let registry = registry.clone();
                async_std::task::spawn_local(run_tunnel_connection(sub_connection, registry));
            }
            Err(e) => {
                log::error!("recv sub_connection error: {}", e);
                break;
            }
        }
    }
}
