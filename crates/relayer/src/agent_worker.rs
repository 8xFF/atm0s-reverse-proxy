use std::{error::Error, marker::PhantomData, time::Instant};

use futures::{select, AsyncWriteExt, FutureExt};
use metrics::{counter, gauge, histogram};
use protocol::stream::pipeline_streams;

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
    rx: async_std::channel::Receiver<P>,
    incoming_conn_handler: H,
}

impl<AG, P, S, H> AgentWorker<AG, P, S, H>
where
    AG: AgentConnection<S>,
    P: ProxyTunnel,
    S: AgentSubConnection,
    H: AgentConnectionHandler<S>,
{
    pub fn new(connection: AG, incoming_conn_handler: H) -> (Self, async_std::channel::Sender<P>) {
        let (tx, rx) = async_std::channel::bounded(3);
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

    async fn handle_proxy_tunnel(&mut self, proxy_tunnel_conn: P) -> Result<(), Box<dyn Error>> {
        counter!(METRICS_TUNNEL_AGENT_COUNT).increment(1);
        let started = Instant::now();
        let mut to_agent_conn = self.connection.create_sub_connection().await?;

        async_std::task::spawn(async move {
            let is_local = proxy_tunnel_conn.local();
            let domain = proxy_tunnel_conn.domain().to_string();
            let handshake = proxy_tunnel_conn.handshake();
            let err1 = to_agent_conn
                .write_all(&(handshake.len() as u16).to_be_bytes())
                .await;
            let err2 = to_agent_conn.write_all(handshake).await;
            if let Err(e) = err1.and(err2) {
                log::error!("handshake for domain {domain} failed {:?}", e);
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
            log::info!("start proxy tunnel for domain {domain}");

            match pipeline_streams(proxy_tunnel_conn, to_agent_conn).await {
                Ok(res) => {
                    log::info!("end proxy tunnel for domain {domain}, res {res:?}");
                }
                Err(e) => {
                    log::error!("end proxy tunnel for domain {domain} err {e:?}");
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

    async fn handle_agent_connection(&mut self, conn: S) -> Result<(), Box<dyn Error>> {
        counter!(METRICS_PROXY_AGENT_COUNT).increment(1);
        let handler = self.incoming_conn_handler.clone();
        let domain = self.connection.domain();
        async_std::task::spawn(async move {
            gauge!(METRICS_PROXY_AGENT_LIVE).increment(1.0);
            log::info!("handle agent connection with external logic");
            if let Err(e) = handler.handle(&domain, conn).await {
                counter!(METRICS_PROXY_AGENT_ERROR_COUNT).increment(1);
                log::error!("handle agent connection error {:?}", e);
            }
            gauge!(METRICS_PROXY_AGENT_LIVE).decrement(1.0);
        });
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let incoming = select! {
            conn = self.rx.recv().fuse() => IncomingConn::FromProxy(conn?),
            conn = self.connection.recv().fuse() => IncomingConn::FromAgent(conn?),
        };

        match incoming {
            IncomingConn::FromProxy(conn) => {
                log::info!("incoming connect request from proxy {}", conn.name());
                if let Err(e) = self.handle_proxy_tunnel(conn).await {
                    log::error!("handle proxy tunnel error {:?}", e);
                    counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
                }
            }
            IncomingConn::FromAgent(conn) => {
                log::info!("incoming connect request from client {}", conn.name());
                if let Err(e) = self.handle_agent_connection(conn).await {
                    log::error!("handle agent connection error {:?}", e);
                    counter!(METRICS_PROXY_AGENT_ERROR_COUNT).increment(1);
                }
            }
        }
        Ok(())
    }
}
