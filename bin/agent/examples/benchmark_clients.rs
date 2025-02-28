use std::str::FromStr;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use argh::FromArgs;
#[cfg(feature = "quic")]
use atm0s_reverse_proxy_agent::QuicConnection;
#[cfg(feature = "tcp")]
use atm0s_reverse_proxy_agent::TcpConnection;
#[cfg(feature = "tls")]
use atm0s_reverse_proxy_agent::TlsConnection;
use atm0s_reverse_proxy_agent::{run_tunnel_connection, Connection, Protocol, ServiceRegistry, SimpleServiceRegistry, SubConnection};
#[cfg(any(feature = "quic", feature = "tls"))]
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use log::LevelFilter;
use picolog::PicoLogger;
#[cfg(any(feature = "quic", feature = "tls"))]
use protocol::DEFAULT_TUNNEL_CERT;
use protocol_ed25519::AgentLocalKey;
#[cfg(feature = "quic")]
use rustls::pki_types::CertificateDer;
use tokio::time::sleep;
#[cfg(feature = "tls")]
use tokio_native_tls::native_tls::Certificate;
use url::Url;

/// A benchmark util for simulating multiple clients connect to relay server
#[derive(FromArgs, Debug, Clone)]
struct Args {
    /// address of relay server
    #[argh(option)]
    connector_addr: Url,

    /// protocol of relay server
    #[argh(option)]
    connector_protocol: Protocol,

    /// http proxy dest
    #[argh(option, default = "SocketAddr::from_str(\"127.0.0.1:8080\").unwrap()")]
    http_dest: SocketAddr,

    /// sni-https proxy dest
    #[argh(option, default = "SocketAddr::from_str(\"127.0.0.1:8443\").unwrap()")]
    https_dest: SocketAddr,

    #[cfg(feature = "quic")]
    /// custom quic server cert in base64
    #[argh(option)]
    custom_quic_cert_base64: Option<String>,

    #[cfg(feature = "quic")]
    /// allow connect in insecure mode
    #[argh(option)]
    allow_quic_insecure: bool,

    #[cfg(feature = "tls")]
    /// custom tls server cert in base64
    #[argh(option)]
    custom_tls_cert_base64: Option<String>,

    #[cfg(feature = "tls")]
    /// allow connect in insecure mode
    #[argh(option)]
    allow_tls_insecure: bool,

    /// clients
    #[argh(option)]
    clients: usize,

    /// wait time between connect action
    #[argh(option, default = "1000")]
    connect_wait_ms: u64,
}

#[tokio::main]
async fn main() {
    let args: Args = argh::from_env();

    #[cfg(feature = "quic")]
    rustls::crypto::ring::default_provider().install_default().expect("should install ring as default");

    //if RUST_LOG env is not set, set it to info
    let level = match std::env::var("RUST_LOG") {
        Ok(v) => LevelFilter::from_str(&v).unwrap_or(LevelFilter::Info),
        _ => LevelFilter::Info,
    };
    PicoLogger::new(level).init();

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
    #[cfg(feature = "quic")]
    let default_tunnel_cert = CertificateDer::from(DEFAULT_TUNNEL_CERT.to_vec());

    #[cfg(feature = "quic")]
    let quic_certs = if let Some(cert) = args.custom_quic_cert_base64 {
        vec![CertificateDer::from(URL_SAFE.decode(&cert).expect("Custom cert should in base64 format").to_vec())]
    } else {
        vec![default_tunnel_cert]
    };
    #[cfg(feature = "tls")]
    let tls_cert = if let Some(cert) = args.custom_tls_cert_base64 {
        Certificate::from_der(&URL_SAFE.decode(cert).expect("Custom cert should in base64 format")).expect("should load custom cert")
    } else {
        Certificate::from_der(DEFAULT_TUNNEL_CERT).expect("should load default cert")
    };
    let agent_signer = AgentLocalKey::random();

    loop {
        log::info!("Connecting to connector... {:?} addr: {}", args.connector_protocol, args.connector_addr);
        let started = Instant::now();
        match args.connector_protocol {
            #[cfg(feature = "tcp")]
            Protocol::Tcp => match TcpConnection::new(args.connector_addr.clone(), &agent_signer).await {
                Ok(conn) => {
                    log::info!("Connected to connector via tcp with res {:?}", conn.response());
                    println!("{client} connected after {:?}", started.elapsed());
                    run_connection_loop(conn, registry.clone()).await;
                }
                Err(e) => {
                    log::error!("Connect to connector via tcp error: {e}");
                }
            },
            #[cfg(feature = "tls")]
            Protocol::Tls => match TlsConnection::new(args.connector_addr.clone(), &agent_signer, tls_cert.clone(), args.allow_tls_insecure).await {
                Ok(conn) => {
                    log::info!("Connected to connector via tcp with res {:?}", conn.response());
                    println!("{client} connected after {:?}", started.elapsed());
                    run_connection_loop(conn, registry.clone()).await;
                }
                Err(e) => {
                    log::error!("Connect to connector via tcp error: {e}");
                }
            },
            #[cfg(feature = "quic")]
            Protocol::Quic => match QuicConnection::new(args.connector_addr.clone(), &agent_signer, &quic_certs, args.allow_quic_insecure).await {
                Ok(conn) => {
                    log::info!("Connected to connector via quic with res {:?}", conn.response());
                    println!("{client} connected after {:?}", started.elapsed());
                    run_connection_loop(conn, registry.clone()).await;
                }
                Err(e) => {
                    log::error!("Connect to connector via quic error: {e}");
                }
            },
        }
        //TODO exponential backoff
        sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn run_connection_loop<S>(mut connection: impl Connection<S>, registry: Arc<dyn ServiceRegistry>)
where
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
