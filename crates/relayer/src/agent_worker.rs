use std::{error::Error, marker::PhantomData, sync::Arc};

use futures::{select, AsyncRead, AsyncWrite, FutureExt};
use metrics::gauge;

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
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin,
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
        let sub_connection = self.connection.create_sub_connection().await?;
        async_std::task::spawn(async move {
            gauge!(crate::METRICS_PROXY_LIVE).increment(1.0);
            let domain = conn.domain().to_string();
            log::info!("start proxy tunnel for domain {}", domain);
            let (mut reader1, mut writer1) = sub_connection.split();
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
            gauge!(crate::METRICS_PROXY_LIVE).increment(-1.0);
        });
        Ok(())
    }

    async fn handle_agent_connection(&mut self, conn: S) -> Result<(), Box<dyn Error>> {
        self.incoming_conn_handler
            .handle(&self.connection.domain(), conn)
            .await?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let incoming = select! {
            conn = self.rx.recv().fuse() => IncommingConn::FromProxy(conn?),
            conn = self.connection.recv().fuse() => IncommingConn::FromAgent(conn?, Default::default()),
        };

        match incoming {
            IncommingConn::FromProxy(conn) => {
                self.handle_proxy_tunnel(conn).await?;
            }
            IncommingConn::FromAgent(conn, _) => {
                self.handle_agent_connection(conn).await?;
            }
        }
        Ok(())
    }
}
