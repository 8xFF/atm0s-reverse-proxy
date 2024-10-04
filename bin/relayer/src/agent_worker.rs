use std::{marker::PhantomData, time::Instant};

use metrics::{counter, gauge, histogram};
use protocol::cluster::write_buf;
use tokio::io::copy_bidirectional;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};

enum IncomingConn<P: ProxyTunnel, S: AgentSubConnection> {
    FromProxy(P),
    FromAgent(S),
}

use crate::{
    agent_listener::{AgentConnection, AgentConnectionHandler, AgentSubConnection},
    proxy_listener::ProxyTunnel,
    METRICS_PROXY_AGENT_COUNT, METRICS_PROXY_AGENT_ERROR_COUNT, METRICS_PROXY_AGENT_LIVE,
    METRICS_PROXY_CLUSTER_ERROR_COUNT, METRICS_PROXY_CLUSTER_LIVE, METRICS_PROXY_HTTP_ERROR_COUNT,
    METRICS_PROXY_HTTP_LIVE, METRICS_TUNNEL_AGENT_COUNT, METRICS_TUNNEL_AGENT_ERROR_COUNT,
    METRICS_TUNNEL_AGENT_HISTOGRAM, METRICS_TUNNEL_AGENT_LIVE,
};

pub struct AgentWorker<AG, P, S, H>
where
    AG: AgentConnection<S>,
    P: ProxyTunnel,
    S: AgentSubConnection,
    H: AgentConnectionHandler<S>,
{
    _tmp: PhantomData<S>,
    connection: AG,
    rx: Receiver<P>,
    incoming_conn_handler: H,
}

impl<AG, P, S, H> AgentWorker<AG, P, S, H>
where
    AG: AgentConnection<S>,
    P: ProxyTunnel,
    S: AgentSubConnection,
    H: AgentConnectionHandler<S>,
{
    pub fn new(connection: AG, incoming_conn_handler: H) -> (Self, Sender<P>) {
        let (tx, rx) = channel(3);
        (
            Self {
                _tmp: PhantomData,
                connection,
                rx,
                incoming_conn_handler,
            },
            tx,
        )
    }

    async fn handle_proxy_tunnel(&mut self, mut proxy_tunnel_conn: P) -> anyhow::Result<()> {
        counter!(METRICS_TUNNEL_AGENT_COUNT).increment(1);
        let started = Instant::now();
        let mut to_agent_conn = self.connection.create_sub_connection().await?;
        let conn_id = self.connection.conn_id();

        tokio::spawn(async move {
            let is_local = proxy_tunnel_conn.local();
            let domain = proxy_tunnel_conn.domain().to_string();
            let handshake = proxy_tunnel_conn.handshake();
            let err = write_buf::<_, 1000>(&mut to_agent_conn, handshake).await;
            if let Err(e) = err {
                log::error!("[AgentWorker {conn_id}] handshake for domain {domain} failed {e:?}");
                if is_local {
                    gauge!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1.0);
                } else {
                    gauge!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1.0);
                }
                return;
            }
            histogram!(METRICS_TUNNEL_AGENT_HISTOGRAM)
                .record(started.elapsed().as_millis() as f32 / 1000.0);

            if is_local {
                gauge!(METRICS_PROXY_HTTP_LIVE).increment(1.0);
            } else {
                gauge!(METRICS_PROXY_CLUSTER_LIVE).increment(1.0);
            }
            gauge!(METRICS_TUNNEL_AGENT_LIVE).increment(1.0);
            log::info!("[AgentWorker {conn_id}] start proxy tunnel for domain {domain}");

            match copy_bidirectional(&mut proxy_tunnel_conn, &mut to_agent_conn).await {
                Ok(res) => {
                    log::info!(
                        "[AgentWorker {conn_id}] end proxy tunnel for domain {domain}, res {res:?}"
                    );
                }
                Err(e) => {
                    log::error!(
                        "[AgentWorker {conn_id}] end proxy tunnel for domain {domain} err {e:?}"
                    );
                }
            }

            if is_local {
                gauge!(METRICS_PROXY_HTTP_LIVE).decrement(1.0);
            } else {
                gauge!(METRICS_PROXY_CLUSTER_LIVE).decrement(1.0);
            }
            gauge!(METRICS_TUNNEL_AGENT_LIVE).decrement(1.0);
        });
        Ok(())
    }

    async fn handle_agent_connection(&mut self, conn: S) -> anyhow::Result<()> {
        counter!(METRICS_PROXY_AGENT_COUNT).increment(1);
        let handler = self.incoming_conn_handler.clone();
        let domain = self.connection.domain();
        let conn_id = self.connection.conn_id();
        tokio::spawn(async move {
            gauge!(METRICS_PROXY_AGENT_LIVE).increment(1.0);
            log::info!("[AgentWorker {conn_id}] handle agent connection with external logic");
            if let Err(e) = handler.handle(&domain, conn).await {
                counter!(METRICS_PROXY_AGENT_ERROR_COUNT).increment(1);
                log::error!("[AgentWorker {conn_id}] handle agent connection error {e:?}");
            }
            gauge!(METRICS_PROXY_AGENT_LIVE).decrement(1.0);
        });
        Ok(())
    }

    pub async fn run(&mut self) -> anyhow::Result<Option<()>> {
        let incoming = select! {
            conn = self.rx.recv() => match conn {
                Some(conn) => IncomingConn::FromProxy(conn),
                None => {
                    log::warn!("[AgentWorker {}] server closed connection", self.connection.conn_id());
                    return Ok(None);
                }
            },
            conn = self.connection.recv() => IncomingConn::FromAgent(conn?),
        };

        match incoming {
            IncomingConn::FromProxy(conn) => {
                log::info!(
                    "[AgentWorker {}] incoming connect request from proxy",
                    self.connection.conn_id()
                );
                if let Err(e) = self.handle_proxy_tunnel(conn).await {
                    log::error!(
                        "[AgentWorker {}] handle proxy tunnel error {e:?}",
                        self.connection.conn_id()
                    );
                    counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
                }
            }
            IncomingConn::FromAgent(conn) => {
                log::info!(
                    "[AgentWorker {}] incoming connect request from client",
                    self.connection.conn_id()
                );
                if let Err(e) = self.handle_agent_connection(conn).await {
                    log::error!(
                        "[AgentWorker {}] handle agent connection error {e:?}",
                        self.connection.conn_id()
                    );
                    counter!(METRICS_PROXY_AGENT_ERROR_COUNT).increment(1);
                }
            }
        }
        Ok(Some(()))
    }
}
