use std::{error::Error, marker::PhantomData, sync::Arc, time::Instant};

use futures::{select, AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt};
use metrics::{counter, gauge, histogram};

enum IncommingConn<
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin,
> {
    FromProxy(Box<dyn ProxyTunnel>),
    FromAgent(S, PhantomData<(R, W)>),
}

use crate::{
    agent_listener::{AgentConnection, AgentConnectionHandler, AgentSubConnection},
    proxy_listener::ProxyTunnel,
    METRICS_PROXY_AGENT_COUNT, METRICS_PROXY_AGENT_ERROR_COUNT, METRICS_PROXY_AGENT_LIVE,
    METRICS_PROXY_CLUSTER_ERROR_COUNT, METRICS_PROXY_CLUSTER_LIVE, METRICS_PROXY_HTTP_ERROR_COUNT,
    METRICS_PROXY_HTTP_LIVE, METRICS_TUNNEL_AGENT_COUNT, METRICS_TUNNEL_AGENT_ERROR_COUNT,
    METRICS_TUNNEL_AGENT_HISTOGRAM, METRICS_TUNNEL_AGENT_LIVE,
};

pub struct AgentWorker<AG, S, R, W>
where
    AG: AgentConnection<S, R, W>,
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    _tmp: PhantomData<(S, R, W)>,
    connection: AG,
    rx: async_std::channel::Receiver<Box<dyn ProxyTunnel>>,
    incoming_conn_handler: Arc<dyn AgentConnectionHandler<S, R, W>>,
}

impl<AG, S, R, W> AgentWorker<AG, S, R, W>
where
    AG: AgentConnection<S, R, W>,
    S: AgentSubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(
        connection: AG,
        incoming_conn_handler: Arc<dyn AgentConnectionHandler<S, R, W>>,
    ) -> (Self, async_std::channel::Sender<Box<dyn ProxyTunnel>>) {
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

    async fn handle_proxy_tunnel(
        &mut self,
        mut conn: Box<dyn ProxyTunnel>,
    ) -> Result<(), Box<dyn Error>> {
        counter!(METRICS_TUNNEL_AGENT_COUNT).increment(1);
        let started = Instant::now();
        let sub_connection = self.connection.create_sub_connection().await?;

        async_std::task::spawn(async move {
            let domain = conn.domain().to_string();
            let (mut reader1, mut writer1) = sub_connection.split();
            if let Some(handshake) = conn.handshake() {
                let err1 = writer1
                    .write_all(&(handshake.len() as u16).to_be_bytes())
                    .await;
                let err2 = writer1.write_all(handshake).await;
                if let Err(e) = err1.and(err2) {
                    log::error!("handshake for domain {domain} failed {:?}", e);
                    if conn.local() {
                        gauge!(METRICS_PROXY_HTTP_ERROR_COUNT).increment(1.0);
                    } else {
                        gauge!(METRICS_PROXY_CLUSTER_ERROR_COUNT).increment(1.0);
                    }
                    return;
                }
            }
            histogram!(METRICS_TUNNEL_AGENT_HISTOGRAM)
                .record(started.elapsed().as_millis() as f32 / 1000.0);

            if conn.local() {
                gauge!(METRICS_PROXY_HTTP_LIVE).increment(1.0);
            } else {
                gauge!(METRICS_PROXY_CLUSTER_LIVE).increment(1.0);
            }
            gauge!(METRICS_TUNNEL_AGENT_LIVE).increment(1.0);
            log::info!("start proxy tunnel for domain {domain}");
            let (mut reader2, mut writer2) = conn.split();

            let job1 = futures::io::copy(&mut reader1, &mut writer2);
            let job2 = futures::io::copy(&mut reader2, &mut writer1);

            select! {
                e = job1.fuse() => {
                    if let Err(e) = e {
                        log::info!("agent => proxy error: {}", e);
                    }
                }
                e = job2.fuse() => {
                    if let Err(e) = e {
                        log::info!("proxy => agent error: {}", e);
                    }
                }
            }

            log::info!("end proxy tunnel for domain {}", domain);
            if conn.local() {
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
            conn = self.rx.recv().fuse() => IncommingConn::FromProxy(conn?),
            conn = self.connection.recv().fuse() => IncommingConn::FromAgent(conn?, Default::default()),
        };

        match incoming {
            IncommingConn::FromProxy(conn) => {
                if let Err(e) = self.handle_proxy_tunnel(conn).await {
                    log::error!("handle proxy tunnel error {:?}", e);
                    counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT).increment(1);
                }
            }
            IncommingConn::FromAgent(conn, _) => {
                if let Err(e) = self.handle_agent_connection(conn).await {
                    log::error!("handle agent connection error {:?}", e);
                    counter!(METRICS_PROXY_AGENT_ERROR_COUNT).increment(1);
                }
            }
        }
        Ok(())
    }
}
