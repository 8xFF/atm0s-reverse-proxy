use std::{marker::PhantomData, net::SocketAddr, sync::Arc, time::Instant};

use futures::StreamExt;
use metrics::histogram;
use protocol::key::{ClusterRequest, ClusterValidator};
use protocol::proxy::AgentId;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::{
    net::TcpListener,
    select,
    sync::mpsc::{channel, Receiver, Sender},
};
use tokio_rustls::TlsAcceptor;
use tokio_yamux::{Session, StreamHandle};

use crate::agent::{AgentSession, AgentSessionControl};
use crate::{AgentSessionId, METRICS_AGENT_HISTOGRAM};

use super::{AgentListener, AgentListenerEvent};

pub type TunnelTlsStream = StreamHandle;
pub struct AgentTlsListener<VALIDATE, HANDSHAKE: ClusterRequest> {
    tls_acceptor: Arc<TlsAcceptor>,
    validate: Arc<VALIDATE>,
    listener: TcpListener,
    internal_tx: Sender<AgentListenerEvent<HANDSHAKE::Context, TunnelTlsStream>>,
    internal_rx: Receiver<AgentListenerEvent<HANDSHAKE::Context, TunnelTlsStream>>,
    _tmp: PhantomData<HANDSHAKE>,
}

impl<VALIDATE, HANDSHAKE: ClusterRequest> AgentTlsListener<VALIDATE, HANDSHAKE> {
    pub async fn new(addr: SocketAddr, validate: VALIDATE, key: PrivatePkcs8KeyDer<'static>, cert: CertificateDer<'static>) -> anyhow::Result<Self> {
        log::info!("[AgentTls] starting with addr {addr}");
        let (internal_tx, internal_rx) = channel(10);
        let config = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(vec![cert], PrivateKeyDer::Pkcs8(key))?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(config));

        Ok(Self {
            tls_acceptor: Arc::new(tls_acceptor),
            listener: TcpListener::bind(addr).await?,
            internal_tx,
            internal_rx,
            validate: validate.into(),
            _tmp: PhantomData,
        })
    }
}

impl<VALIDATE: ClusterValidator<REQ>, REQ: DeserializeOwned + Send + Sync + 'static + ClusterRequest> AgentListener<REQ::Context, TunnelTlsStream> for AgentTlsListener<VALIDATE, REQ> {
    async fn recv(&mut self) -> anyhow::Result<AgentListenerEvent<REQ::Context, TunnelTlsStream>> {
        loop {
            let (stream, remote) = select! {
                incoming = self.listener.accept() => incoming?,
                event = self.internal_rx.recv() => break Ok(event.expect("should receive event from internal channel")),
            };

            let tls_acceptor = self.tls_acceptor.clone();
            let validate = self.validate.clone();
            let internal_tx = self.internal_tx.clone();
            tokio::spawn(async move {
                if let Err(e) = run_connection(validate, tls_acceptor, stream, remote, internal_tx).await {
                    log::error!("[AgentTls] connection {remote} error {e:?}");
                }
            });
        }
    }

    async fn shutdown(&mut self) {}
}

async fn run_connection<VALIDATE: ClusterValidator<REQ>, REQ: ClusterRequest>(
    validate: Arc<VALIDATE>,
    tls_acceptor: Arc<TlsAcceptor>,
    stream: TcpStream,
    remote: SocketAddr,
    internal_tx: Sender<AgentListenerEvent<REQ::Context, TunnelTlsStream>>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    log::info!("[AgentTls] new connection from {remote}, handshaking tls");
    let mut in_stream = tls_acceptor.accept(stream).await?;
    log::info!("[AgentTls] new connection from {remote}, handshake tls success");

    let mut buf = [0u8; 4096];
    let buf_len = in_stream.read(&mut buf).await?;

    log::info!("[AgentTls] new connection from {remote} got handhsake data {buf_len} bytes");

    let req = validate.validate_connect_req(&buf[0..buf_len])?;
    let domain = validate.generate_domain(&req)?;
    let agent_id = AgentId::try_from_domain(&domain)?;
    let session_id = AgentSessionId::rand();
    let agent_ctx = req.context();

    log::info!("[AgentTls] new connection from {remote} validated with domain {domain} agent_id: {agent_id}, session uuid: {session_id}");

    let res_buf = validate.sign_response_res(&req, None);
    in_stream.write_all(&res_buf).await?;
    let (control_tx, mut control_rx) = channel(10);

    internal_tx
        .send(AgentListenerEvent::Connected(agent_id, AgentSession::new(agent_id, session_id, domain, control_tx)))
        .await
        .expect("should send to main loop");

    log::info!("[AgentTls] new connection {agent_id} {session_id}  started loop");
    let mut session = Session::new_client(in_stream, Default::default());
    histogram!(METRICS_AGENT_HISTOGRAM).record(started.elapsed().as_millis() as f32 / 1000.0);

    loop {
        select! {
            control = control_rx.recv() => match control {
                Some(control) => match control {
                    AgentSessionControl::CreateStream(tx) => {
                        log::info!("[AgentTls] agent {agent_id} {session_id} create stream request");
                        match session.open_stream() {
                            Ok(stream) => {
                                log::info!("[AgentTls] agent {agent_id} {session_id} created stream");
                                if let Err(_e) = tx.send(Ok(stream)) {
                                    log::error!("[AgentTls] agent {agent_id} {session_id}  send created stream error");
                                }
                            },
                            Err(err) => {
                                if let Err(_e) = tx.send(Err(err.into())) {
                                    log::error!("[AgentTls] agent {agent_id} {session_id}  send create stream's error, may be internal channel failed");
                                }
                            },
                        }
                    },
                },
                None => {
                    break;
                }
            },
            accept = session.next() => match accept {
                Some(Ok(stream)) => {
                    let internal_tx = internal_tx.clone();
                    let agent_ctx = agent_ctx.clone();
                    tokio::spawn(async move {
                        internal_tx.send(AgentListenerEvent::IncomingStream(agent_id, agent_ctx, stream)).await.expect("should send to main loop");
                    });
                },
                Some(Err(err)) => {
                    log::error!("[AgentTls] agent {agent_id} {session_id} Tcp connection error {err:?}");
                    break;
                },
                None => {
                    log::error!("[AgentTls] agent {agent_id} {session_id} Tcp connection broken with None");
                    break;
                }
            }
        }
    }

    log::info!("[AgentTls] agent {agent_id} {session_id}  stopped loop");

    internal_tx.send(AgentListenerEvent::Disconnected(agent_id, session_id)).await.expect("should send to main loop");

    Ok(())
}
