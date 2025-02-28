use std::{collections::HashMap, net::SocketAddr, time::Instant};

use ::metrics::{counter, gauge, histogram};
use agent::{
    quic::AgentQuicListener,
    tcp::{AgentTcpListener, TunnelTcpStream},
    tls::{AgentTlsListener, TunnelTlsStream},
    AgentListener, AgentListenerEvent, AgentSession,
};
use anyhow::anyhow;
use p2p::{
    alias_service::{AliasGuard, AliasService, AliasServiceRequester},
    HandshakeProtocol, P2pNetwork, P2pNetworkConfig, P2pService, P2pServiceEvent, P2pServiceRequester, PeerAddress, PeerId,
};
use protocol::{
    cluster::{write_object, AgentTunnelRequest},
    key::{ClusterRequest, ClusterValidator},
    proxy::{AgentId, ProxyDestination},
};
use quic::TunnelQuicStream;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde::de::DeserializeOwned;
use tokio::{
    io::{copy_bidirectional, AsyncRead, AsyncWrite},
    select,
};

mod agent;
mod metrics;
mod proxy;
mod quic;

pub use agent::AgentSessionId;
pub use metrics::*;
pub use p2p;
pub use proxy::{http::HttpDestinationDetector, rtsp::RtspDestinationDetector, tls::TlsDestinationDetector, ProxyDestinationDetector, ProxyTcpListener};

const ALIAS_SERVICE: u16 = 0;
const PROXY_TO_AGENT_SERVICE: u16 = 1;
const TUNNEL_TO_CLUSTER_SERVICE: u16 = 2;

#[derive(Clone)]
pub struct TunnelServiceCtx {
    pub service: P2pServiceRequester,
    pub alias: AliasServiceRequester,
}

/// This service take care how we process a incoming request from agent
pub trait TunnelServiceHandle<Ctx> {
    fn start(&mut self, _ctx: &TunnelServiceCtx);
    fn on_agent_conn<S: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(&mut self, _ctx: &TunnelServiceCtx, _agent_id: AgentId, ctx: Ctx, _stream: S);
    fn on_cluster_event(&mut self, _ctx: &TunnelServiceCtx, _event: P2pServiceEvent);
}

pub struct QuicRelayerConfig<SECURE, TSH> {
    pub agent_unsecure_listener: SocketAddr,
    pub agent_secure_listener: SocketAddr,
    pub proxy_http_listener: SocketAddr,
    pub proxy_tls_listener: SocketAddr,
    pub proxy_rtsp_listener: SocketAddr,
    pub proxy_rtsps_listener: SocketAddr,

    pub agent_key: PrivatePkcs8KeyDer<'static>,
    pub agent_cert: CertificateDer<'static>,

    pub sdn_peer_id: PeerId,
    pub sdn_listener: SocketAddr,
    pub sdn_seeds: Vec<PeerAddress>,
    pub sdn_key: PrivatePkcs8KeyDer<'static>,
    pub sdn_cert: CertificateDer<'static>,
    pub sdn_advertise_address: Option<SocketAddr>,
    pub sdn_secure: SECURE,

    pub tunnel_service_handle: TSH,
}

pub enum QuicRelayerEvent {
    AgentConnected(AgentId, AgentSessionId, String),
    AgentDisconnected(AgentId, AgentSessionId),
    Continue,
}

pub struct QuicRelayer<SECURE, VALIDATE, REQ: ClusterRequest, TSH> {
    agent_quic: AgentQuicListener<VALIDATE, REQ>,
    agent_tcp: AgentTcpListener<VALIDATE, REQ>,
    agent_tls: AgentTlsListener<VALIDATE, REQ>,
    http_proxy: ProxyTcpListener<HttpDestinationDetector>,
    tls_proxy: ProxyTcpListener<TlsDestinationDetector>,
    rtsp_proxy: ProxyTcpListener<RtspDestinationDetector>,
    rtsps_proxy: ProxyTcpListener<TlsDestinationDetector>,

    sdn: P2pNetwork<SECURE>,

    sdn_alias_requester: AliasServiceRequester,
    // This service is for proxy from internet to agent
    sdn_proxy_service: P2pService,
    // This service is for tunnel from agent to outside
    sdn_tunnel_service: P2pService,
    tunnel_service_ctx: TunnelServiceCtx,
    tunnel_service_handle: TSH,

    agent_quic_sessions: HashMap<AgentId, HashMap<AgentSessionId, (AgentSession<TunnelQuicStream>, AliasGuard)>>,
    agent_tcp_sessions: HashMap<AgentId, HashMap<AgentSessionId, (AgentSession<TunnelTcpStream>, AliasGuard)>>,
    agent_tls_sessions: HashMap<AgentId, HashMap<AgentSessionId, (AgentSession<TunnelTlsStream>, AliasGuard)>>,
}

impl<SECURE, VALIDATE, REQ: ClusterRequest, TSH> QuicRelayer<SECURE, VALIDATE, REQ, TSH>
where
    SECURE: HandshakeProtocol,
    VALIDATE: ClusterValidator<REQ>,
    REQ: DeserializeOwned + Send + Sync + 'static,
    TSH: TunnelServiceHandle<REQ::Context> + Send + Sync + 'static,
{
    pub async fn new(mut cfg: QuicRelayerConfig<SECURE, TSH>, validate: VALIDATE) -> anyhow::Result<Self> {
        let mut sdn = P2pNetwork::new(P2pNetworkConfig {
            peer_id: cfg.sdn_peer_id,
            listen_addr: cfg.sdn_listener,
            advertise: cfg.sdn_advertise_address.map(|a| a.into()),
            priv_key: cfg.sdn_key,
            cert: cfg.sdn_cert,
            tick_ms: 1000,
            seeds: cfg.sdn_seeds,
            secure: cfg.sdn_secure,
        })
        .await?;

        let mut sdn_alias = AliasService::new(sdn.create_service(ALIAS_SERVICE.into()));
        let sdn_alias_requester = sdn_alias.requester();
        tokio::spawn(async move { while sdn_alias.run_loop().await.is_ok() {} });
        let sdn_proxy_service = sdn.create_service(PROXY_TO_AGENT_SERVICE.into());
        let sdn_tunnel_service = sdn.create_service(TUNNEL_TO_CLUSTER_SERVICE.into());
        let tunnel_service_ctx = TunnelServiceCtx {
            service: sdn_tunnel_service.requester(),
            alias: sdn_alias_requester.clone(),
        };
        cfg.tunnel_service_handle.start(&tunnel_service_ctx);

        Ok(Self {
            agent_quic: AgentQuicListener::new(cfg.agent_secure_listener, cfg.agent_key.clone_key(), cfg.agent_cert.clone(), validate.clone()).await?,
            agent_tcp: AgentTcpListener::new(cfg.agent_unsecure_listener, validate.clone()).await?,
            agent_tls: AgentTlsListener::new(cfg.agent_secure_listener, validate, cfg.agent_key, cfg.agent_cert).await?,
            http_proxy: ProxyTcpListener::new(cfg.proxy_http_listener, Default::default()).await?,
            tls_proxy: ProxyTcpListener::new(cfg.proxy_tls_listener, Default::default()).await?,
            rtsp_proxy: ProxyTcpListener::new(cfg.proxy_rtsp_listener, Default::default()).await?,
            rtsps_proxy: ProxyTcpListener::new(cfg.proxy_rtsps_listener, Default::default()).await?,

            sdn,
            sdn_alias_requester,
            sdn_proxy_service,
            sdn_tunnel_service,
            tunnel_service_handle: cfg.tunnel_service_handle,
            tunnel_service_ctx,

            agent_quic_sessions: HashMap::new(),
            agent_tcp_sessions: HashMap::new(),
            agent_tls_sessions: HashMap::new(),
        })
    }

    fn process_proxy<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(&mut self, proxy: T, dest: ProxyDestination, is_from_cluster: bool) {
        let agent_id = match dest.agent_id() {
            Ok(agent_id) => agent_id,
            Err(e) => {
                log::warn!("[QuicRelayer] proxy to {dest:?} failed to get agent id: {e}");
                return;
            }
        };
        if let Some(sessions) = self.agent_tcp_sessions.get(&agent_id) {
            let session = sessions.values().next().expect("should have session");
            let job = proxy_local_to_agent(is_from_cluster, proxy, dest, session.0.clone());
            tokio::spawn(async move {
                if let Err(e) = job.await {
                    counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
                    counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
                    log::error!("[QuicRelayer {agent_id}] proxy to agent error {:?}", e);
                };
            });
        } else if let Some(sessions) = self.agent_tls_sessions.get(&agent_id) {
            let session = sessions.values().next().expect("should have session");
            let job = proxy_local_to_agent(is_from_cluster, proxy, dest, session.0.clone());
            tokio::spawn(async move {
                if let Err(e) = job.await {
                    counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
                    counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
                    log::error!("[QuicRelayer {agent_id}] proxy to agent error {:?}", e);
                };
            });
        } else if let Some(sessions) = self.agent_quic_sessions.get(&agent_id) {
            let session = sessions.values().next().expect("should have session");
            let job = proxy_local_to_agent(is_from_cluster, proxy, dest, session.0.clone());
            tokio::spawn(async move {
                if let Err(e) = job.await {
                    counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
                    counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
                    log::error!("[QuicRelayer {agent_id}] proxy to agent error {:?}", e);
                };
            });
        } else if !is_from_cluster {
            // we don't allow two times tunnel over cluster
            let sdn_requester = self.sdn_proxy_service.requester();
            let job = proxy_to_cluster(proxy, dest, self.sdn_alias_requester.clone(), sdn_requester);
            tokio::spawn(async move {
                if let Err(e) = job.await {
                    counter!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1);
                    counter!(METRICS_TUNNEL_CLUSTER_ERROR_COUNT).increment(1);
                    log::error!("[QuicRelayer {agent_id}] proxy to cluster error {:?}", e);
                };
            });
        } else {
            log::warn!("[QuicRelayer {agent_id}] proxy to {dest:?} not match any kind");
            counter!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1);
            counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
        }
    }

    pub fn p2p(&mut self) -> &mut P2pNetwork<SECURE> {
        &mut self.sdn
    }

    pub async fn recv(&mut self) -> anyhow::Result<QuicRelayerEvent> {
        select! {
            tunnel = self.http_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest, false);
                Ok(QuicRelayerEvent::Continue)
            },
            tunnel = self.tls_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest, false);
                Ok(QuicRelayerEvent::Continue)
            },
            tunnel = self.rtsp_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest, false);
                Ok(QuicRelayerEvent::Continue)
            },
            tunnel = self.rtsps_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest, false);
                Ok(QuicRelayerEvent::Continue)
            },
            _ = self.sdn.recv() =>  {
                Ok(QuicRelayerEvent::Continue)
            },
            event = self.agent_quic.recv() => process_incoming_event::<_, _, REQ>(event?, &self.sdn_alias_requester, &mut self.agent_quic_sessions, &mut self.tunnel_service_handle, &self.tunnel_service_ctx),
            event = self.agent_tcp.recv() => process_incoming_event::<_, _, REQ>(event?, &self.sdn_alias_requester, &mut self.agent_tcp_sessions, &mut self.tunnel_service_handle, &self.tunnel_service_ctx),
            event = self.agent_tls.recv() => process_incoming_event::<_, _, REQ>(event?, &self.sdn_alias_requester, &mut self.agent_tls_sessions, &mut self.tunnel_service_handle, &self.tunnel_service_ctx),
            event = self.sdn_proxy_service.recv() => match event.expect("sdn channel crash") {
                P2pServiceEvent::Unicast(from, ..) => {
                    log::warn!("[QuicRelayer] proxy service don't accept unicast msg from {from}");
                    Ok(QuicRelayerEvent::Continue)
                },
                P2pServiceEvent::Broadcast(from, ..) => {
                    log::warn!("[QuicRelayer] proxy service don't accept broadcast msg from {from}");
                    Ok(QuicRelayerEvent::Continue)
                },
                P2pServiceEvent::Stream(_from, meta, stream) => {
                    if let Ok(proxy_dest) = bincode::deserialize::<ProxyDestination>(&meta) {
                        self.process_proxy(stream, proxy_dest, true);
                    }
                    Ok(QuicRelayerEvent::Continue)
                },
            },
            event = self.sdn_tunnel_service.recv() => {
                self.tunnel_service_handle.on_cluster_event(&self.tunnel_service_ctx, event.expect("sdn channel crash"));
                Ok(QuicRelayerEvent::Continue)
            },
            _ = tokio::signal::ctrl_c() => {
                log::info!("[QuicRelayer] shutdown inprogress");
                self.sdn.shutdown();
                self.agent_quic.shutdown().await;

                log::info!("[QuicRelayer] shutdown done");
                Ok(QuicRelayerEvent::Continue)
            }
        }
    }
}

fn process_incoming_event<S, TSH, REQ>(
    event: AgentListenerEvent<REQ::Context, S>,
    alias_requester: &AliasServiceRequester,
    sessions: &mut HashMap<AgentId, HashMap<AgentSessionId, (AgentSession<S>, AliasGuard)>>,
    tunnel_service_handle: &mut TSH,
    tunnel_service_ctx: &TunnelServiceCtx,
) -> anyhow::Result<QuicRelayerEvent>
where
    S: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    REQ: ClusterRequest,
    TSH: TunnelServiceHandle<REQ::Context> + Send + Sync + 'static,
{
    match event {
        AgentListenerEvent::Connected(agent_id, agent_session) => {
            counter!(METRICS_AGENT_COUNT).increment(1);
            log::info!("[QuicRelayer] agent {agent_id} {} connected", agent_session.session_id());
            let session_id = agent_session.session_id();
            let domain = agent_session.domain().to_owned();
            let alias = alias_requester.register(*agent_id);
            sessions.entry(agent_id).or_default().insert(agent_session.session_id(), (agent_session, alias));
            gauge!(METRICS_AGENT_LIVE).increment(1.0);
            Ok(QuicRelayerEvent::AgentConnected(agent_id, session_id, domain))
        }
        AgentListenerEvent::IncomingStream(agent_id, agent_ctx, stream) => {
            tunnel_service_handle.on_agent_conn(tunnel_service_ctx, agent_id, agent_ctx, stream);
            Ok(QuicRelayerEvent::Continue)
        }
        AgentListenerEvent::Disconnected(agent_id, session_id) => {
            log::info!("[QuicRelayer] agent {agent_id} {session_id} disconnected");
            if let Some(child_sessions) = sessions.get_mut(&agent_id) {
                child_sessions.remove(&session_id);
                if child_sessions.is_empty() {
                    log::info!("[QuicRelayer] agent disconnected all connections {agent_id} {session_id}");
                    sessions.remove(&agent_id);
                }
                gauge!(METRICS_AGENT_LIVE).decrement(1.0);
            }
            Ok(QuicRelayerEvent::AgentDisconnected(agent_id, session_id))
        }
    }
}

async fn proxy_local_to_agent<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static, S: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(
    is_from_cluster: bool,
    mut proxy: T,
    dest: ProxyDestination,
    agent: AgentSession<S>,
) -> anyhow::Result<()> {
    let agent_id = agent.agent_id();
    let started = Instant::now();
    if is_from_cluster {
        counter!(METRICS_PROXY_CLUSTER_COUNT).increment(1);
    } else {
        counter!(METRICS_PROXY_HTTP_COUNT).increment(1);
    }
    counter!(METRICS_TUNNEL_AGENT_COUNT).increment(1);
    log::info!("[ProxyLocal {agent_id}] creating stream to agent");
    let mut stream = agent.create_stream().await?;

    histogram!(METRICS_TUNNEL_AGENT_HISTOGRAM).record(started.elapsed().as_millis() as f32 / 1000.0);
    log::info!("[ProxyLocal {agent_id}] created stream to agent => writing connect request");
    write_object::<_, _, 500>(
        &mut stream,
        &AgentTunnelRequest {
            service: dest.service,
            tls: dest.tls,
            domain: dest.domain,
        },
    )
    .await?;

    log::info!("[ProxyLocal {agent_id}] proxy data with agent ...");

    gauge!(METRICS_TUNNEL_AGENT_LIVE).increment(1.0);
    if is_from_cluster {
        gauge!(METRICS_PROXY_CLUSTER_LIVE).increment(1.0);
    } else {
        gauge!(METRICS_PROXY_HTTP_LIVE).increment(1.0);
    }
    match copy_bidirectional(&mut proxy, &mut stream).await {
        Ok(res) => {
            log::info!("[ProxyLocal {agent_id}] proxy data with agent done with res {res:?}");
        }
        Err(e) => {
            log::error!("[ProxyLocal {agent_id}] proxy data with agent error {e}");
        }
    };

    if is_from_cluster {
        gauge!(METRICS_PROXY_CLUSTER_LIVE).decrement(1.0);
    } else {
        gauge!(METRICS_PROXY_HTTP_LIVE).decrement(1.0);
    }
    gauge!(METRICS_TUNNEL_AGENT_LIVE).decrement(1.0);

    Ok(())
}

async fn proxy_to_cluster<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(
    mut proxy: T,
    dest: ProxyDestination,
    alias_requeser: AliasServiceRequester,
    sdn_requester: P2pServiceRequester,
) -> anyhow::Result<()> {
    let started = Instant::now();
    counter!(METRICS_PROXY_HTTP_COUNT).increment(1);
    counter!(METRICS_TUNNEL_CLUSTER_COUNT).increment(1);
    let agent_id = dest.agent_id()?;
    log::info!("[ProxyCluster {agent_id}] finding location of agent {agent_id}");
    let found_location = alias_requeser.find(*agent_id).await.ok_or(anyhow!("ALIAS_NOT_FOUND"))?;
    let dest_node = match found_location {
        p2p::alias_service::AliasFoundLocation::Local => return Err(anyhow!("wrong alias context, cluster shouldn't in local")),
        p2p::alias_service::AliasFoundLocation::Hint(dest) => dest,
        p2p::alias_service::AliasFoundLocation::Scan(dest) => dest,
    };
    log::info!("[ProxyCluster {agent_id}] found location of agent {agent_id}: {found_location:?} => opening cluster connection to {dest_node}");

    let meta = bincode::serialize(&dest).expect("should convert ProxyDestination to bytes");

    let mut stream = sdn_requester.open_stream(dest_node, meta).await?;
    histogram!(METRICS_TUNNEL_CLUSTER_HISTOGRAM).record(started.elapsed().as_millis() as f32 / 1000.0);

    log::info!("[ProxyCluster {agent_id}] proxy over {dest_node} ...");
    gauge!(METRICS_TUNNEL_CLUSTER_LIVE).increment(1.0);
    gauge!(METRICS_PROXY_HTTP_LIVE).increment(1.0);

    match copy_bidirectional(&mut proxy, &mut stream).await {
        Ok(res) => {
            log::info!("[ProxyCluster {agent_id}] proxy over {dest_node} done with res {res:?}");
        }
        Err(e) => {
            log::error!("[ProxyCluster {agent_id}] proxy over {dest_node} error {e}");
        }
    }

    gauge!(METRICS_PROXY_HTTP_LIVE).decrement(1.0);
    gauge!(METRICS_TUNNEL_CLUSTER_LIVE).decrement(1.0);
    Ok(())
}
