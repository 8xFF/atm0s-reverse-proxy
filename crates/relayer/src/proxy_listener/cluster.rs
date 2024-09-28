use std::{
    error::Error,
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use atm0s_sdn::{
    features::{
        alias::{self, FoundLocation},
        socket, FeaturesControl, FeaturesEvent,
    },
    sans_io_runtime::backend::PollingBackend,
    secure::StaticKeyAuthorization,
    services::visualization,
    NodeAddr, NodeId, SdnBuilder, SdnControllerUtils, SdnExtIn, SdnExtOut, SdnOwner,
    ServiceBroadcastLevel,
};
use futures::{AsyncRead, AsyncWrite};
use protocol::{
    cluster::{wait_object, write_object, ClusterTunnelRequest, ClusterTunnelResponse},
    stream::NamedStream,
};
use quinn::{Endpoint, Incoming, RecvStream, SendStream};

use alias_async::AliasAsyncEvent;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    uptime: u32,
}
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
    builder.add_service(Arc::new(service::RelayServiceBuilder));
    builder.set_authorization(StaticKeyAuthorization::new(&secret_key));

    for seed in seeds {
        builder.add_seed(seed);
    }

    async_std::task::spawn(async move {
        let node_info = NodeInfo { uptime: 0 };
        let started_at = Instant::now();
        let mut count = 0;
        let mut controller =
            builder.build::<PollingBackend<SdnOwner, 128, 128>>(workers, node_info);
        while controller.process().is_some() {
            if count % 500 == 0 {
                //each 5 seconds
                controller.service_control(
                    visualization::SERVICE_ID.into(),
                    (),
                    visualization::Control::UpdateInfo(NodeInfo {
                        uptime: started_at.elapsed().as_secs() as u32,
                    }),
                );
                count = 0;
            }
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
                            pkt.data,
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
            count += 1;
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

impl ProxyListener for ProxyClusterListener {
    type Stream = ProxyClusterIncommingTunnel;
    async fn recv(&mut self) -> Option<Self::Stream> {
        let connecting = self.server.accept().await?;
        log::info!("incoming connection from {}", connecting.remote_address());
        Some(ProxyClusterIncommingTunnel {
            virtual_addr: connecting.remote_address(),
            domain: "".to_string(),
            handshake: vec![],
            connecting: Some(connecting),
            streams: None,
            wait_stream_read: None,
            wait_stream_write: None,
        })
    }
}

pub struct ProxyClusterIncommingTunnel {
    virtual_addr: SocketAddr,
    domain: String,
    handshake: Vec<u8>,
    connecting: Option<Incoming>,
    streams: Option<(RecvStream, SendStream)>,
    wait_stream_read: Option<Waker>,
    wait_stream_write: Option<Waker>,
}

impl ProxyTunnel for ProxyClusterIncommingTunnel {
    fn source_addr(&self) -> String {
        format!("sdn-quic://{}", self.virtual_addr)
    }

    async fn wait(&mut self) -> Option<()> {
        let connecting = self.connecting.take()?;
        let connection = connecting.await.ok()?;
        log::info!(
            "[ProxyClusterTunnel] incoming connection from: {}",
            connection.remote_address()
        );
        let (mut send, mut recv) = connection.accept_bi().await.ok()?;
        log::info!(
            "[ProxyClusterTunnel] accepted bi stream from: {}",
            connection.remote_address()
        );
        let req = wait_object::<_, ClusterTunnelRequest, 1000>(&mut recv)
            .await
            .ok()?;
        write_object::<_, _, 1000>(&mut send, ClusterTunnelResponse { success: true })
            .await
            .ok()?;
        log::info!("[ProxyClusterTunnel] got domain: {}", req.domain);

        self.domain = req.domain;
        self.handshake = req.handshake;
        self.streams = Some((recv, send));
        if let Some(waker) = self.wait_stream_read.take() {
            waker.wake();
        }
        if let Some(waker) = self.wait_stream_write.take() {
            waker.wake();
        }
        Some(())
    }
    fn local(&self) -> bool {
        false
    }
    fn domain(&self) -> &str {
        &self.domain
    }
    fn handshake(&self) -> &[u8] {
        &self.handshake
    }
}

impl NamedStream for ProxyClusterIncommingTunnel {
    fn name(&self) -> &'static str {
        "proxy-sdn-quic-tunnel"
    }
}

impl AsyncRead for ProxyClusterIncommingTunnel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.streams {
            Some((ref mut read, _)) => Pin::new(read).poll_read(cx, buf),
            None => {
                this.wait_stream_read = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

impl AsyncWrite for ProxyClusterIncommingTunnel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.streams {
            Some((_, ref mut write)) => Pin::new(write).poll_write(cx, buf).map_err(|e| e.into()),
            None => {
                this.wait_stream_write = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.streams {
            Some((_, ref mut write)) => Pin::new(write).poll_flush(cx),
            None => {
                this.wait_stream_write = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.streams {
            Some((_, ref mut write)) => Pin::new(write).poll_close(cx),
            None => {
                this.wait_stream_write = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

pub struct ProxyClusterOutgoingTunnel {
    send: SendStream,
    recv: RecvStream,
}

impl ProxyClusterOutgoingTunnel {
    pub async fn connect<'a>(
        socket: VirtualUdpSocket,
        dest: Ipv4Addr,
        domain: &str,
        server_certs: &[CertificateDer<'a>],
    ) -> Result<Self, Box<dyn Error>> {
        let client = make_quinn_client(socket, server_certs)?;
        log::info!(
            "[ProxyClusterOutgoingTunnel] connecting to agent for domain: {domain} in node {dest}"
        );
        let connecting = client.connect(SocketAddr::V4(SocketAddrV4::new(dest, 443)), "cluster")?;
        let connection = connecting.await?;
        log::info!(
            "[ProxyClusterOutgoingTunnel]  connected to agent for domain: {domain} in node {dest}"
        );
        let (send, recv) = connection.open_bi().await?;
        log::info!(
            "[ProxyClusterOutgoingTunnel] opened bi stream to agent for domain: {domain} in node {dest}"
        );
        Ok(Self { send, recv })
    }
}

impl NamedStream for ProxyClusterOutgoingTunnel {
    fn name(&self) -> &'static str {
        "outgoing-sdn-quic-tunnel"
    }
}

impl AsyncRead for ProxyClusterOutgoingTunnel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.recv).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProxyClusterOutgoingTunnel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.send)
            .poll_write(cx, buf)
            .map_err(|e| e.into())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.send).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.send).poll_close(cx)
    }
}
