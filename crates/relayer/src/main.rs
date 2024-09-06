use atm0s_reverse_proxy_relayer::{
    run_agent_connection, run_sdn, tunnel_task, AgentIncomingConnHandlerDummy, AgentListener,
    AgentQuicListener, AgentStore, AgentTcpListener, HttpDomainDetector, NetworkVisualizeEvent,
    ProxyListener, ProxyTcpListener, RtspDomainDetector, TlsDomainDetector, TunnelContext,
    METRICS_AGENT_COUNT, METRICS_AGENT_HISTOGRAM, METRICS_AGENT_LIVE, METRICS_PROXY_AGENT_COUNT,
    METRICS_PROXY_AGENT_ERROR_COUNT, METRICS_PROXY_AGENT_HISTOGRAM, METRICS_PROXY_AGENT_LIVE,
    METRICS_PROXY_CLUSTER_COUNT, METRICS_PROXY_CLUSTER_ERROR_COUNT, METRICS_PROXY_CLUSTER_LIVE,
    METRICS_PROXY_HTTP_COUNT, METRICS_PROXY_HTTP_ERROR_COUNT, METRICS_PROXY_HTTP_LIVE,
    METRICS_TUNNEL_AGENT_COUNT, METRICS_TUNNEL_AGENT_ERROR_COUNT, METRICS_TUNNEL_AGENT_HISTOGRAM,
    METRICS_TUNNEL_AGENT_LIVE, METRICS_TUNNEL_CLUSTER_COUNT, METRICS_TUNNEL_CLUSTER_ERROR_COUNT,
    METRICS_TUNNEL_CLUSTER_HISTOGRAM, METRICS_TUNNEL_CLUSTER_LIVE,
};
use atm0s_sdn::{NodeAddr, NodeId};
use clap::Parser;
#[cfg(feature = "expose-metrics")]
use metrics_dashboard::{build_dashboard_route, DashboardOptions};
#[cfg(feature = "expose-metrics")]
use poem::{listener::TcpListener, middleware::Tracing, EndpointExt as _, Route, Server};
use protocol::{
    services::SERVICE_RTSP, DEFAULT_CLUSTER_CERT, DEFAULT_CLUSTER_KEY, DEFAULT_TUNNEL_CERT,
    DEFAULT_TUNNEL_KEY,
};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
#[cfg(feature = "expose-metrics")]
use std::net::{Ipv4Addr, SocketAddrV4};
use std::{net::SocketAddr, process::exit, sync::Arc};

use futures::{select, FutureExt};
use metrics::{describe_counter, describe_gauge, describe_histogram};
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt, EnvFilter};

/// A HTTP and SNI HTTPs proxy for expose your local service to the internet.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// API port
    #[arg(env, long, default_value_t = 33334)]
    api_port: u16,

    /// Http proxy port
    #[arg(env, long, default_value_t = 80)]
    http_port: u16,

    /// Sni-https proxy port
    #[arg(env, long, default_value_t = 443)]
    https_port: u16,

    /// Rtsp proxy port
    #[arg(env, long, default_value_t = 554)]
    rtsp_port: u16,

    /// Sni-rtsp proxy port
    #[arg(env, long, default_value_t = 5443)]
    rtsps_port: u16,

    /// Number of times to greet
    #[arg(env, long, default_value = "0.0.0.0:33333")]
    connector_port: SocketAddr,

    /// Root domain
    #[arg(env, long, default_value = "localtunnel.me")]
    root_domain: String,

    /// atm0s-sdn node-id
    #[arg(env, long)]
    sdn_node_id: NodeId,

    /// atm0s-sdn node-id
    #[arg(env, long, default_value_t = 0)]
    sdn_port: u16,

    /// atm0s-sdn secret key
    #[arg(env, long, default_value = "insecure")]
    sdn_secret_key: String,

    /// atm0s-sdn seed address
    #[arg(env, long)]
    sdn_seeds: Vec<NodeAddr>,

    /// atm0s-sdn workers
    #[arg(env, long, default_value_t = 1)]
    sdn_workers: usize,

    #[arg(env, long)]
    sdn_visualize: bool,
}

#[async_std::main]
async fn main() {
    let default_tunnel_cert = CertificateDer::from(DEFAULT_TUNNEL_CERT.to_vec());
    let default_tunnel_key = PrivatePkcs8KeyDer::from(DEFAULT_TUNNEL_KEY.to_vec());

    let default_cluster_cert = CertificateDer::from(DEFAULT_CLUSTER_CERT.to_vec());
    let default_cluster_key = PrivatePkcs8KeyDer::from(DEFAULT_CLUSTER_KEY.to_vec());

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("should install ring as default");
    let args = Args::parse();
    let cluster_validator = protocol_ed25519::ClusterValidatorImpl::new(args.root_domain);

    //if RUST_LOG env is not set, set it to info
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    let mut quic_agent_listener = AgentQuicListener::new(
        args.connector_port,
        cluster_validator.clone(),
        default_tunnel_key,
        default_tunnel_cert,
    )
    .await;
    let mut tcp_agent_listener =
        AgentTcpListener::new(args.connector_port, cluster_validator).await;
    let mut proxy_http_listener = ProxyTcpListener::new(
        args.http_port,
        false,
        None,
        Arc::new(HttpDomainDetector::default()),
    )
    .await
    .expect("Should listen http port");
    let mut proxy_tls_listener = ProxyTcpListener::new(
        args.https_port,
        true,
        None,
        Arc::new(TlsDomainDetector::default()),
    )
    .await
    .expect("Should listen tls port");
    let mut proxy_rtsp_listener = ProxyTcpListener::new(
        args.rtsp_port,
        false,
        Some(SERVICE_RTSP),
        Arc::new(RtspDomainDetector::default()),
    )
    .await
    .expect("Should listen rtsp port");
    let mut proxy_rtsps_listener = ProxyTcpListener::new(
        args.rtsps_port,
        true,
        Some(SERVICE_RTSP),
        Arc::new(TlsDomainDetector::default()),
    )
    .await
    .expect("Should listen rtsps port");
    let agents = AgentStore::default();

    #[cfg(feature = "expose-metrics")]
    let app = Route::new()
        .nest(
            "/dashboard/",
            build_dashboard_route(DashboardOptions {
                custom_charts: vec![],
                include_default: true,
            }),
        )
        .with(Tracing);

    // this is for online agent counting
    describe_gauge!(METRICS_AGENT_LIVE, "Live agent count");
    describe_histogram!(
        METRICS_AGENT_HISTOGRAM,
        "Incoming agent connection accept time histogram"
    );
    describe_counter!(METRICS_AGENT_COUNT, "Number of connected agents");

    // this is for proxy from agent counting (incoming)
    describe_gauge!(
        METRICS_PROXY_AGENT_LIVE,
        "Live incoming proxy from agent to cluster"
    );
    describe_counter!(
        METRICS_PROXY_AGENT_COUNT,
        "Number of incoming proxy from agent to cluster"
    );
    describe_histogram!(
        METRICS_PROXY_AGENT_HISTOGRAM,
        "Incoming proxy from agent to cluster latency histogram"
    );
    describe_counter!(
        METRICS_PROXY_AGENT_ERROR_COUNT,
        "Number of incoming proxy error from agent to cluster"
    );

    // this is for http proxy counting (incoming)
    describe_gauge!(METRICS_PROXY_HTTP_LIVE, "Live incoming http proxy");
    describe_counter!(METRICS_PROXY_HTTP_COUNT, "Number of incoming http proxy");
    describe_counter!(
        METRICS_PROXY_HTTP_ERROR_COUNT,
        "Number of incoming http proxy error"
    );

    // this is for cluster proxy (incoming)
    describe_gauge!(METRICS_PROXY_CLUSTER_LIVE, "Live incoming cluster proxy");
    describe_counter!(
        METRICS_PROXY_CLUSTER_COUNT,
        "Number of incoming cluster proxy"
    );
    describe_counter!(
        METRICS_PROXY_CLUSTER_ERROR_COUNT,
        "Number of incoming cluster proxy error"
    );

    // this is for tunnel from local node to other node (outgoing)
    describe_gauge!(
        METRICS_TUNNEL_CLUSTER_LIVE,
        "Live outgoing tunnel to cluster"
    );
    describe_counter!(
        METRICS_TUNNEL_CLUSTER_COUNT,
        "Number of outgoing tunnel to cluster"
    );
    describe_histogram!(
        METRICS_TUNNEL_CLUSTER_HISTOGRAM,
        "Outgoing tunnel to cluster latency histogram"
    );
    describe_counter!(
        METRICS_TUNNEL_CLUSTER_ERROR_COUNT,
        "Number of outgoing tunnel to cluster error"
    );

    // this is for tunnel from local node to agent  (outgoing)
    describe_gauge!(METRICS_TUNNEL_AGENT_LIVE, "Live outgoing tunnel to agent");
    describe_counter!(
        METRICS_TUNNEL_AGENT_COUNT,
        "Number of outgoing tunnel to agent"
    );
    describe_counter!(
        METRICS_TUNNEL_AGENT_HISTOGRAM,
        "Outgoing tunnel to agent latency histogram"
    );
    describe_counter!(
        METRICS_TUNNEL_AGENT_ERROR_COUNT,
        "Number of outgoing tunnel to agent error"
    );

    #[cfg(feature = "expose-metrics")]
    let api_port = args.api_port;
    #[cfg(feature = "expose-metrics")]
    async_std::task::spawn(async move {
        log::warn!("Started api server");
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, api_port);
        let _ = Server::new(TcpListener::bind(SocketAddr::V4(addr)))
            .name("relay-metrics")
            .run(app)
            .await;
    });
    let sdn_addrs = local_ip_address::list_afinet_netifas()
        .expect("Should have list interfaces")
        .into_iter()
        .filter(|(_, ip)| {
            if ip.is_unspecified() || ip.is_multicast() {
                false
            } else {
                std::net::UdpSocket::bind(SocketAddr::new(*ip, 0)).is_ok()
            }
        })
        .map(|(_name, ip)| SocketAddr::new(ip, args.sdn_port))
        .collect::<Vec<_>>();

    let (mut cluster_endpoint, alias_sdk, mut virtual_net, mut network_visualize) = run_sdn(
        args.sdn_node_id,
        &sdn_addrs,
        args.sdn_secret_key,
        args.sdn_seeds,
        args.sdn_workers,
        default_cluster_key,
        default_cluster_cert.clone(),
        args.sdn_visualize,
    )
    .await;

    if args.sdn_visualize {
        async_std::task::spawn(async move {
            while let Some(e) = network_visualize.pop_request().await {
                match e {
                    NetworkVisualizeEvent::Snapshot(nodes) => {
                        log::debug!("Snapshot: {:?}", nodes);
                    }
                    NetworkVisualizeEvent::NodeChanged(node_id, node_info, conns) => {
                        log::debug!("NodeChanged: {:?} {:?} {:?}", node_id, node_info, conns);
                    }
                    NetworkVisualizeEvent::NodeRemoved(node_id) => {
                        log::debug!("NodeRemoved: {:?}", node_id);
                    }
                }
            }
        });
    }

    let agent_rpc_handler_quic = Arc::new(AgentIncomingConnHandlerDummy::default());
    let agent_rpc_handler_tcp = Arc::new(AgentIncomingConnHandlerDummy::default());

    loop {
        select! {
            e = quic_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    let agents1 = agents.clone();
                    let alias_sdk1 = alias_sdk.clone();
                    let agent_rpc_handler_quic1 = agent_rpc_handler_quic.clone();
                    async_std::task::spawn(async move {
                        run_agent_connection(agent_connection, agents1, alias_sdk1, agent_rpc_handler_quic1).await;
                    });
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = tcp_agent_listener.recv().fuse() => match e {
                Ok(agent_connection) => {
                    let agents1 = agents.clone();
                    let alias_sdk1 = alias_sdk.clone();
                    let agent_rpc_handler_tcp1 = agent_rpc_handler_tcp.clone();
                    async_std::task::spawn(async move {
                        run_agent_connection(agent_connection, agents1, alias_sdk1, agent_rpc_handler_tcp1).await;
                    });
                }
                Err(e) => {
                    log::error!("agent_listener error {}", e);
                    exit(1);
                }
            },
            e = proxy_http_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    if let Some(socket) = virtual_net.udp_socket(0).await {
                        async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(alias_sdk.clone(), socket, vec![default_cluster_cert.clone()])));
                    } else {
                        log::error!("Virtual Net create socket error");
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_tls_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    if let Some(socket) = virtual_net.udp_socket(0).await {
                        async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(alias_sdk.clone(), socket, vec![default_cluster_cert.clone()])));
                    } else {
                        log::error!("Virtual Net create socket error");
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_rtsps_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    if let Some(socket) = virtual_net.udp_socket(0).await {
                        async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(alias_sdk.clone(), socket, vec![default_cluster_cert.clone()])));
                    } else {
                        log::error!("Virtual Net create socket error");
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = proxy_rtsp_listener.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    if let Some(socket) = virtual_net.udp_socket(0).await {
                        async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Local(alias_sdk.clone(), socket, vec![default_cluster_cert.clone()])));
                    } else {
                        log::error!("Virtual Net create socket error");
                    }
                }
                None => {
                    log::error!("proxy_http_listener.recv()");
                    exit(2);
                }
            },
            e = cluster_endpoint.recv().fuse() => match e {
                Some(proxy_tunnel) => {
                    async_std::task::spawn(tunnel_task(proxy_tunnel, agents.clone(), TunnelContext::Cluster));
                }
                None => {
                    log::error!("cluster_endpoint.accept()");
                    exit(3);
                }
            },
            e = virtual_net.recv().fuse() => match e {
                Some(()) => {}
                None => {
                    log::error!("virtual_net.recv()");
                    exit(4);
                }
            },
        }
    }
}
