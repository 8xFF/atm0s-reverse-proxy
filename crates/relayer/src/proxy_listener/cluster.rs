use std::{net::SocketAddr, sync::Arc, time::Duration};

use atm0s_sdn::{
    features::{
        alias::{self, FoundLocation},
        socket, FeaturesControl, FeaturesEvent,
    },
    sans_io_runtime::backend::PollingBackend,
    secure::StaticKeyAuthorization,
    services::visualization,
    NodeAddr, NodeId, SdnBuilder, SdnExtIn, SdnExtOut, SdnOwner, ServiceBroadcastLevel,
};
use futures::{AsyncRead, AsyncWrite};
use protocol::cluster::{ClusterTunnelRequest, ClusterTunnelResponse};
use quinn::{Endpoint, Incoming};

use alias_async::AliasAsyncEvent;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use vnet::{NetworkPkt, OutEvent};

use super::{ProxyListener, ProxyTunnel};

mod alias_async;
mod quinn_utils;
mod service;
mod vnet;
mod vsocket;

pub use alias_async::AliasSdk;
pub use quinn_utils::{make_quinn_client, make_quinn_server};
pub use vnet::VirtualNetwork;
pub use vsocket::VirtualUdpSocket;

type UserData = ();
type NodeInfo = ();
type SC = visualization::Control<NodeInfo>;
type SE = visualization::Event<NodeInfo>;
type TC = ();
type TW = ();

pub async fn run_sdn(
    node_id: NodeId,
    sdn_addrs: &[SocketAddr],
    secret_key: String,
    seeds: Vec<NodeAddr>,
    workers: usize,
    priv_key: PrivatePkcs8KeyDer<'static>,
    cert: CertificateDer<'static>,
) -> (ProxyClusterListener, AliasSdk, VirtualNetwork) {
    let (mut vnet, tx, rx) = vnet::VirtualNetwork::new(node_id);
    let (mut alias_async, alias_sdk) = alias_async::AliasAsync::new();

    let server_socket = vnet
        .udp_socket(443)
        .await
        .expect("Should have 443 virtual port");

    let mut builder =
        SdnBuilder::<UserData, SC, SE, TC, TW, NodeInfo>::new(node_id, sdn_addrs, vec![]);

    builder.set_manual_discovery(vec!["tunnel".to_string()], vec!["tunnel".to_string()]);
    builder.set_visualization_collector(false);
    builder.add_service(Arc::new(service::RelayServiceBuilder::default()));
    builder.set_authorization(StaticKeyAuthorization::new(&secret_key));

    for seed in seeds {
        builder.add_seed(seed);
    }

    async_std::task::spawn(async move {
        let mut controller = builder.build::<PollingBackend<SdnOwner, 128, 128>>(workers, ());
        while controller.process().is_some() {
            while let Ok(c) = rx.try_recv() {
                // log::info!("Command: {:?}", c);
                match c {
                    OutEvent::Bind(port) => {
                        log::info!("Bind port: {}", port);
                        controller.send_to(
                            0,
                            SdnExtIn::FeaturesControl(
                                (),
                                FeaturesControl::Socket(socket::Control::Bind(port)),
                            ),
                        );
                    }
                    OutEvent::Pkt(pkt) => {
                        let send = socket::Control::SendTo(
                            pkt.local_port,
                            pkt.remote,
                            pkt.remote_port,
                            pkt.data.into(),
                            pkt.meta,
                        );
                        controller.send_to(
                            0,
                            SdnExtIn::FeaturesControl((), FeaturesControl::Socket(send)),
                        );
                    }
                    OutEvent::Unbind(port) => {
                        log::info!("Unbind port: {}", port);
                        controller.send_to(
                            0,
                            SdnExtIn::FeaturesControl(
                                (),
                                FeaturesControl::Socket(socket::Control::Unbind(port)),
                            ),
                        );
                    }
                }
            }
            while let Some(event) = alias_async.pop_request() {
                let control = match event {
                    AliasAsyncEvent::Query(alias) => alias::Control::Query {
                        alias,
                        service: service::SERVICE_ID,
                        level: ServiceBroadcastLevel::Global,
                    },
                    AliasAsyncEvent::Register(alias) => alias::Control::Register {
                        alias,
                        service: service::SERVICE_ID,
                        level: ServiceBroadcastLevel::Global,
                    },
                    AliasAsyncEvent::Unregister(alias) => alias::Control::Unregister { alias },
                };
                controller.send_to(
                    0,
                    SdnExtIn::FeaturesControl((), FeaturesControl::Alias(control)),
                );
            }
            while let Some(event) = controller.pop_event() {
                match event {
                    SdnExtOut::FeaturesEvent(
                        _,
                        FeaturesEvent::Socket(socket::Event::RecvFrom(
                            local_port,
                            remote,
                            remote_port,
                            data,
                            meta,
                        )),
                    ) => {
                        if let Err(e) = tx.try_send(NetworkPkt {
                            local_port,
                            remote,
                            remote_port,
                            data,
                            meta,
                        }) {
                            log::error!("Failed to send to tx: {:?}", e);
                        }
                    }
                    SdnExtOut::FeaturesEvent(
                        _,
                        FeaturesEvent::Alias(alias::Event::QueryResult(alias, res)),
                    ) => {
                        log::info!("FeaturesEvent::Alias: {alias} {:?}", res);
                        let res = res.map(|a| match a {
                            FoundLocation::Local => node_id,
                            FoundLocation::Notify(node) => node,
                            FoundLocation::CachedHint(node) => node,
                            FoundLocation::RemoteHint(node) => node,
                            FoundLocation::RemoteScan(node) => node,
                        });
                        alias_async.push_response(alias, res);
                    }
                    _ => {}
                }
            }
            async_std::task::sleep(Duration::from_millis(1)).await;
        }
    });

    (
        ProxyClusterListener::new(server_socket, priv_key, cert),
        alias_sdk,
        vnet,
    )
}

pub struct ProxyClusterListener {
    server: Endpoint,
}

impl ProxyClusterListener {
    pub fn new(
        socket: VirtualUdpSocket,
        priv_key: PrivatePkcs8KeyDer<'static>,
        cert: CertificateDer<'static>,
    ) -> Self {
        let server = make_quinn_server(socket, priv_key, cert).expect("");
        Self { server }
    }
}

#[async_trait::async_trait]
impl ProxyListener for ProxyClusterListener {
    async fn recv(&mut self) -> Option<Box<dyn ProxyTunnel>> {
        let connecting = self.server.accept().await?;
        log::info!("incoming connection from {}", connecting.remote_address());
        Some(Box::new(ProxyClusterTunnel {
            domain: "".to_string(),
            connecting: Some(connecting),
            streams: None,
        }))
    }
}

pub struct ProxyClusterTunnel {
    domain: String,
    connecting: Option<Incoming>,
    streams: Option<(
        Box<dyn AsyncRead + Send + Sync + Unpin>,
        Box<dyn AsyncWrite + Send + Sync + Unpin>,
    )>,
}

#[async_trait::async_trait]
impl ProxyTunnel for ProxyClusterTunnel {
    async fn wait(&mut self) -> Option<()> {
        let connecting = self.connecting.take()?;
        let connection = connecting.await.ok()?;
        log::info!("incoming connection from: {}", connection.remote_address());
        let (mut send, mut recv) = connection.accept_bi().await.ok()?;
        log::info!("accepted bi stream from: {}", connection.remote_address());
        let mut req_buf = [0; 1500];
        let req_size = recv.read(&mut req_buf).await.ok()??;
        let req = ClusterTunnelRequest::try_from(&req_buf[..req_size]).ok()?;
        let res_buf: Vec<u8> = (&ClusterTunnelResponse { success: true }).into();
        send.write_all(&res_buf).await.ok()?;
        log::info!("ProxyClusterTunnel domain: {}", req.domain);

        self.domain = req.domain;
        self.streams = Some((Box::new(recv), Box::new(send)));
        Some(())
    }
    fn local(&self) -> bool {
        false
    }
    fn domain(&self) -> &str {
        &self.domain
    }
    fn handshake(&self) -> Option<&[u8]> {
        None
    }
    fn split(
        &mut self,
    ) -> (
        Box<dyn AsyncRead + Send + Sync + Unpin>,
        Box<dyn AsyncWrite + Send + Sync + Unpin>,
    ) {
        self.streams.take().expect("Should has send and recv")
    }
}
