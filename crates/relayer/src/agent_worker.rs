use std::{marker::PhantomData, error::Error};

use futures::{select, AsyncRead, AsyncWrite, FutureExt};
use metrics::increment_gauge;

use crate::{
    agent_listener::{AgentConnection, AgentSubConnection},
    proxy_listener::ProxyTunnel,
};

pub struct AgentWorker<AG, S, R, W, PT, PR, PW>
where
    AG: AgentConnection<S, R, W>,
    S: AgentSubConnection<R, W>,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    PT: ProxyTunnel<PR, PW>,
    PR: AsyncRead + Unpin,
    PW: AsyncWrite + Unpin,
{
    _tmp: PhantomData<(S, R, W, PR, PW)>,
    connection: AG,
    rx: async_std::channel::Receiver<PT>,
}

impl<AG, S, R, W, PT, PR, PW> AgentWorker<AG, S, R, W, PT, PR, PW>
where
    AG: AgentConnection<S, R, W>,
    S: AgentSubConnection<R, W> + 'static,
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin,
    PT: ProxyTunnel<PR, PW> + 'static,
    PR: AsyncRead + Send + Unpin,
    PW: AsyncWrite + Send + Unpin,
{
    pub fn new(connection: AG) -> (Self, async_std::channel::Sender<PT>) {
        let (tx, rx) = async_std::channel::bounded(3);
        (
            Self {
                _tmp: PhantomData,
                connection,
                rx,
            },
            tx,
        )
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let incoming = select! {
            incoming = self.rx.recv().fuse() => incoming?,
            e = self.connection.recv().fuse() => {
                e?;
                return Ok(());
            }
        };
        let sub_connection = self.connection.create_sub_connection().await?;
        async_std::task::spawn(async move {
            increment_gauge!(crate::METRICS_PROXY_LIVE, 1.0);
            let domain = incoming.domain().to_string();
            log::info!("start proxy tunnel for domain {}", domain);
            let (mut reader1, mut writer1) = sub_connection.split();
            let (mut reader2, mut writer2) = incoming.split();

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
            increment_gauge!(crate::METRICS_PROXY_LIVE, -1.0);
        });
        Ok(())
    }
}
