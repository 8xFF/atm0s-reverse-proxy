use std::str::FromStr;
use std::{alloc::System, net::SocketAddr, sync::Arc};

use log::LevelFilter;
use picolog::PicoLogger;

#[cfg(feature = "quic")]
use atm0s_reverse_proxy_agent::QuicConnection;
#[cfg(feature = "tls")]
use atm0s_reverse_proxy_agent::TlsConnection;
use atm0s_reverse_proxy_agent::{run_tunnel_connection, Connection, Protocol, ServiceRegistry, SimpleServiceRegistry, SubConnection, TcpConnection};
#[cfg(any(feature = "quic", feature = "tls"))]
use base64::{engine::general_purpose::URL_SAFE, Engine as _};

use argh::FromArgs;
use protocol::services::SERVICE_RTSP;
#[cfg(any(feature = "quic", feature = "tls"))]
use protocol::DEFAULT_TUNNEL_CERT;
use protocol_ed25519::AgentLocalKey;
#[cfg(feature = "quic")]
use rustls::pki_types::CertificateDer;
use tokio::time::sleep;
#[cfg(feature = "tls")]
use tokio_native_tls::native_tls::Certificate;
use url::Url;

#[global_allocator]
static A: System = System;

/// A HTTP and SNI HTTPs proxy for expose your local service to the internet.
#[derive(FromArgs, Debug)]
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

    /// rtsp proxy dest
    #[argh(option, default = "SocketAddr::from_str(\"127.0.0.1:554\").unwrap()")]
    rtsp_dest: SocketAddr,

    /// sni-https proxy dest
    #[argh(option, default = "SocketAddr::from_str(\"127.0.0.1:5443\").unwrap()")]
    rtsps_dest: SocketAddr,

    /// persistent local key
    #[argh(option, default = "String::from(\"local_key.pem\")")]
    local_key: String,

    #[cfg(feature = "quic")]
    /// custom quic server cert in base64
    #[argh(option)]
    custom_quic_cert_base64: Option<String>,

    #[cfg(feature = "quic")]
    /// allow connect in insecure mode
    #[argh(switch)]
    allow_quic_insecure: bool,

    #[cfg(feature = "tls")]
    /// custom tls server cert in base64
    #[argh(option)]
    custom_tls_cert_base64: Option<String>,

    #[cfg(feature = "tls")]
    /// allow connect in insecure mode
    #[argh(switch)]
    allow_tls_insecure: bool,
}

#[tokio::main]
async fn main() {
    let args: Args = argh::from_env();
    //if RUST_LOG env is not set, set it to info
    let level = match std::env::var("RUST_LOG") {
        Ok(v) => LevelFilter::from_str(&v).unwrap_or(LevelFilter::Info),
        _ => LevelFilter::Info,
    };
    PicoLogger::new(level).init();

    #[cfg(feature = "quic")]
    let quic_certs = if let Some(cert) = args.custom_quic_cert_base64 {
        vec![CertificateDer::from(URL_SAFE.decode(cert).expect("Custom cert should in base64 format").to_vec())]
    } else {
        vec![CertificateDer::from(DEFAULT_TUNNEL_CERT.to_vec())]
    };

    #[cfg(feature = "tls")]
    let tls_cert = if let Some(cert) = args.custom_tls_cert_base64 {
        Certificate::from_der(&URL_SAFE.decode(cert).expect("Custom cert should in base64 format")).expect("should load custom cert")
    } else {
        Certificate::from_der(DEFAULT_TUNNEL_CERT).expect("should load default cert")
    };

    #[cfg(feature = "quic")]
    rustls::crypto::ring::default_provider().install_default().expect("should install ring as default");

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
        log::info!("Connecting to connector... {:?} addr: {}", args.connector_protocol, args.connector_addr);
        match args.connector_protocol {
            #[cfg(feature = "tcp")]
            Protocol::Tcp => match TcpConnection::new(args.connector_addr.clone(), &agent_signer).await {
                Ok(conn) => {
                    log::info!("Connected to connector via tcp with res {:?}", conn.response());
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

pub async fn run_connection_loop<S>(mut connection: impl Connection<S>, registry: Arc<dyn ServiceRegistry>)
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
