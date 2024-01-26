use std::sync::Arc;

use atm0s_sdn::virtual_socket::{create_vnet, make_insecure_quinn_server, VirtualNet};
use atm0s_sdn::{
    convert_enum, KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdk,
    KeyValueSdkEvent, LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent,
    LayersSpreadRouterSyncHandlerEvent, ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent,
    ManualHandlerEvent, NetworkPlane, NetworkPlaneConfig, NodeAddrBuilder, NodeAliasBehavior,
    NodeAliasSdk, PubsubServiceBehaviour, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent,
    SharedRouter, SystemTimer, UdpTransport,
};
use atm0s_sdn::{NodeAddr, NodeId};
use futures::{AsyncRead, AsyncWrite};
use protocol::cluster::{ClusterTunnelRequest, ClusterTunnelResponse};
use quinn::{Connecting, Endpoint};

use super::{ProxyListener, ProxyTunnel};

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
    KeyValue(KeyValueBehaviorEvent),
    Pubsub(PubsubServiceBehaviourEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
    KeyValue(KeyValueHandlerEvent),
    Pubsub(PubsubServiceHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeSdkEvent {
    KeyValue(KeyValueSdkEvent),
}

pub async fn run_sdn(
    node_id: NodeId,
    sdn_port: u16,
    secret_key: String,
    seeds: Vec<NodeAddr>,
) -> (ProxyClusterListener, NodeAliasSdk, VirtualNet) {
    let secure = Arc::new(atm0s_sdn::StaticKeySecure::new(&secret_key));
    let mut node_addr_builder = NodeAddrBuilder::new(node_id);
    let udp_socket = UdpTransport::prepare(sdn_port, &mut node_addr_builder).await;
    let transport = UdpTransport::new(node_addr_builder.addr(), udp_socket, secure.clone());

    let node_addr = node_addr_builder.addr();
    log::info!("atm0s-sdn listen on addr {}", node_addr);

    let timer = Arc::new(SystemTimer());
    let router = SharedRouter::new(node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id,
        node_addr,
        seeds,
        local_tags: vec!["relayer".to_string()],
        connect_tags: vec!["relayer".to_string()],
    });

    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());
    let router = Arc::new(router);

    let kv_sdk = KeyValueSdk::new();
    let kv_behaviour = KeyValueBehavior::new(node_id, 1000, Some(Box::new(kv_sdk)));
    let (pubsub_behavior, pubsub_sdk) = PubsubServiceBehaviour::new(node_id, timer.clone());
    let (node_alias_behavior, node_alias_sdk) = NodeAliasBehavior::new(node_id, pubsub_sdk);
    let (virtual_socket, virtual_socket_sdk) = create_vnet(node_id, router.clone());

    let plan_cfg = NetworkPlaneConfig {
        router,
        node_id,
        tick_ms: 1000,
        behaviors: vec![
            Box::new(manual),
            Box::new(spreads_layer_router),
            Box::new(kv_behaviour),
            Box::new(virtual_socket),
            Box::new(pubsub_behavior),
            Box::new(node_alias_behavior),
        ],
        transport: Box::new(transport),
        timer,
    };

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(plan_cfg);

    async_std::task::spawn(async move {
        plane.started();
        while let Ok(_) = plane.recv().await {}
        plane.stopped();
    });

    (
        ProxyClusterListener::new(&virtual_socket_sdk),
        node_alias_sdk,
        virtual_socket_sdk,
    )
}

pub struct ProxyClusterListener {
    server: Endpoint,
}

impl ProxyClusterListener {
    pub fn new(sdk: &VirtualNet) -> Self {
        let server =
            make_insecure_quinn_server(sdk.create_udp_socket(80, 1000).expect("")).expect("");

        Self { server }
    }
}

#[async_trait::async_trait]
impl ProxyListener for ProxyClusterListener {
    async fn recv(&mut self) -> Option<Box<dyn ProxyTunnel>> {
        Some(Box::new(ProxyClusterTunnel {
            domain: "".to_string(),
            connecting: Some(self.server.accept().await?),
            streams: None,
        }))
    }
}

pub struct ProxyClusterTunnel {
    domain: String,
    connecting: Option<Connecting>,
    streams: Option<(
        Box<dyn AsyncRead + Send + Sync + Unpin>,
        Box<dyn AsyncWrite + Send + Sync + Unpin>,
    )>,
}

#[async_trait::async_trait]
impl ProxyTunnel for ProxyClusterTunnel {
    async fn wait(&mut self) -> Option<()> {
        let connection = self.connecting.take()?.await.ok()?;
        let (mut send, mut recv) = connection.accept_bi().await.ok()?;
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
    fn domain(&self) -> &str {
        &self.domain
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
