use std::{collections::HashMap, net::SocketAddr};

use agent::{
    quic::AgentQuicListener,
    tcp::{AgentTcpListener, TunnelTcpStream},
    AgentId, AgentListener, AgentListenerEvent, AgentSession, AgentSessionId,
};
use anyhow::{anyhow, Ok};
use protocol::key::ClusterValidator;
use proxy::{http::HttpDestinationDetector, rtsp::RtspDestinationDetector, tls::TlsDestinationDetector, ProxyDestination, ProxyTcpListener};
use quic::TunnelQuicStream;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sdn::{NodeId, UnstructedSdn, UnstrutedSdnRequester};
use serde::de::DeserializeOwned;
use tokio::{
    io::{copy_bidirectional, AsyncRead, AsyncWrite},
    select,
};

mod agent;
mod proxy;
mod quic;
mod sdn;

pub struct QuicRelayerConfig {
    pub node: NodeId,
    pub agent_listener: SocketAddr,
    pub sdn_listener: SocketAddr,
    pub proxy_http_listener: SocketAddr,
    pub proxy_tls_listener: SocketAddr,
    pub proxy_rtsp_listener: SocketAddr,
    pub proxy_rtsps_listener: SocketAddr,

    pub agent_key: PrivatePkcs8KeyDer<'static>,
    pub agent_cert: CertificateDer<'static>,

    pub sdn_key: PrivatePkcs8KeyDer<'static>,
    pub sdn_cert: CertificateDer<'static>,
}

pub struct QuicRelayer<VALIDATE, REQ> {
    agent_quic: AgentQuicListener<VALIDATE, REQ>,
    agent_tcp: AgentTcpListener,
    http_proxy: ProxyTcpListener<HttpDestinationDetector>,
    tls_proxy: ProxyTcpListener<TlsDestinationDetector>,
    rtsp_proxy: ProxyTcpListener<RtspDestinationDetector>,
    rtsps_proxy: ProxyTcpListener<TlsDestinationDetector>,
    sdn: UnstructedSdn,

    agent_quic_sessions: HashMap<AgentId, HashMap<AgentSessionId, AgentSession<TunnelQuicStream>>>,
    agent_tcp_sessions: HashMap<AgentId, HashMap<AgentSessionId, AgentSession<TunnelTcpStream>>>,
}

impl<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Send + Sync + 'static> QuicRelayer<VALIDATE, REQ> {
    pub async fn new(cfg: QuicRelayerConfig, validate: VALIDATE) -> anyhow::Result<Self> {
        Ok(Self {
            agent_quic: AgentQuicListener::new(cfg.agent_listener, cfg.agent_key, cfg.agent_cert, validate).await?,
            agent_tcp: AgentTcpListener::new(cfg.agent_listener).await?,
            http_proxy: ProxyTcpListener::new(cfg.proxy_http_listener, Default::default()).await?,
            tls_proxy: ProxyTcpListener::new(cfg.proxy_tls_listener, Default::default()).await?,
            rtsp_proxy: ProxyTcpListener::new(cfg.proxy_rtsp_listener, Default::default()).await?,
            rtsps_proxy: ProxyTcpListener::new(cfg.proxy_rtsps_listener, Default::default()).await?,
            sdn: UnstructedSdn::new(cfg.node, cfg.sdn_listener, cfg.sdn_key, cfg.sdn_cert).await?,

            agent_quic_sessions: HashMap::new(),
            agent_tcp_sessions: HashMap::new(),
        })
    }

    fn process_proxy<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(&mut self, proxy: T, dest: ProxyDestination) {
        let agent_id = dest.agent_id();
        if let Some(sessions) = self.agent_tcp_sessions.get(&agent_id) {
            let session = sessions.values().next().expect("should have session");
            tokio::spawn(proxy_local(proxy, dest, session.clone()));
        } else if let Some(sessions) = self.agent_quic_sessions.get(&agent_id) {
            let session = sessions.values().next().expect("should have session");
            tokio::spawn(proxy_local(proxy, dest, session.clone()));
        } else {
            let sdn_requester = self.sdn.requester();
            tokio::spawn(proxy_cluster(proxy, dest, sdn_requester));
        }
    }

    pub async fn recv(&mut self) -> anyhow::Result<()> {
        select! {
            tunnel = self.http_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest);
                Ok(())
            },
            tunnel = self.tls_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest);
                Ok(())
            },
            tunnel = self.rtsp_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest);
                Ok(())
            },
            tunnel = self.rtsps_proxy.recv() => {
                let (dest, tunnel) = tunnel?;
                self.process_proxy(tunnel, dest);
                Ok(())
            },
            _ = self.sdn.recv() =>  {
                Ok(())
            },
            event = self.agent_quic.recv() => match event? {
                AgentListenerEvent::Connected(agent_id, agent_session) => {
                    log::info!("[QuicRelayer] agent {agent_id} {} connected", agent_session.session_id());
                    self.agent_quic_sessions.entry(agent_id).or_default().insert(agent_session.session_id(), agent_session);
                    Ok(())
                },
                AgentListenerEvent::IncomingStream(agent_id, stream) => {
                    //TODO process here
                    Ok(())
                }
                AgentListenerEvent::Disconnected(agent_id, session_id) => {
                    log::info!("[QuicRelayer] agent {agent_id} {session_id} disconnected");
                    if let Some(sessions) = self.agent_quic_sessions.get_mut(&agent_id) {
                        sessions.remove(&session_id);
                        if sessions.is_empty() {
                            log::info!("[QuicRelayer] agent disconnected all connections {agent_id} {session_id}");
                            self.agent_quic_sessions.remove(&agent_id);
                        }
                    }
                    Ok(())
                },
            },
            event = self.agent_tcp.recv() => match event? {
                AgentListenerEvent::Connected(agent_id, agent_session) => {
                    log::info!("[QuicRelayer] agent {agent_id} {} connected", agent_session.session_id());
                    let entry = self.agent_tcp_sessions.entry(agent_id).or_default();
                    entry.insert(agent_session.session_id(), agent_session);
                    Ok(())
                },
                AgentListenerEvent::IncomingStream(agent_id, stream) => {
                    //TODO process here
                    Ok(())
                }
                AgentListenerEvent::Disconnected(agent_id, session_id) => {
                    log::info!("[QuicRelayer] agent {agent_id} {session_id} disconnected");
                    if let Some(sessions) = self.agent_tcp_sessions.get_mut(&agent_id) {
                        sessions.remove(&session_id);
                        if sessions.is_empty() {
                            log::info!("[QuicRelayer] agent disconnected all connections {agent_id} {session_id}");
                            self.agent_tcp_sessions.remove(&agent_id);
                        }
                    }
                    Ok(())
                },
            }
        }
    }
}

async fn proxy_local<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static, S: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(
    mut proxy: T,
    dest: ProxyDestination,
    agent: AgentSession<S>,
) -> anyhow::Result<()> {
    let mut stream = agent.create_stream().await?;
    let res = copy_bidirectional(&mut proxy, &mut stream).await?;
    log::info!("[ProxyLocal] done with res {res:?}");
    Ok(())
}

async fn proxy_cluster<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static>(mut proxy: T, dest: ProxyDestination, sdn_requester: UnstrutedSdnRequester) -> anyhow::Result<()> {
    let agent_id = dest.agent_id();
    let dest_node = sdn_requester.find_alias(*agent_id).await?.ok_or(anyhow!("ALIAS_NOT_FOUND"))?;
    let mut stream = sdn_requester.create_stream_to(dest_node).await?;
    let res = copy_bidirectional(&mut proxy, &mut stream).await?;
    log::info!("[ProxyCluster] over {dest_node} done with res {res:?}");
    Ok(())
}
