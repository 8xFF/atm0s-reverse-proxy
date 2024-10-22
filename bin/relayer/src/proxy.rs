use std::{net::SocketAddr, sync::Arc};

use protocol::proxy::ProxyDestination;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::mpsc::{channel, Receiver, Sender},
};

pub mod http;
pub mod rtsp;
pub mod tls;

pub trait ProxyDestinationDetector: Send + Sync + 'static {
    fn determine(&self, stream: &mut TcpStream) -> impl std::future::Future<Output = anyhow::Result<ProxyDestination>> + Send;
}

pub struct ProxyTcpListener<Detector> {
    listener: TcpListener,
    detector: Arc<Detector>,
    rx: Receiver<(ProxyDestination, TcpStream)>,
    tx: Sender<(ProxyDestination, TcpStream)>,
}

impl<Detector: ProxyDestinationDetector> ProxyTcpListener<Detector> {
    pub async fn new(addr: SocketAddr, detector: Detector) -> anyhow::Result<Self> {
        let (tx, rx) = channel(10);
        Ok(Self {
            detector: detector.into(),
            listener: TcpListener::bind(addr).await?,
            tx,
            rx,
        })
    }

    pub async fn recv(&mut self) -> anyhow::Result<(ProxyDestination, TcpStream)> {
        loop {
            select! {
                event = self.listener.accept() => {
                    let (mut stream, remote) = event?;
                    let tx = self.tx.clone();
                    let detector = self.detector.clone();
                    tokio::spawn(async move {
                        match detector.determine(&mut stream).await {
                            Ok(destination) => {
                                log::info!("[ProxyTcpListener] determine destination {}, service {:?} for remote {remote}", destination.domain, destination.service);
                                tx.send((destination, stream)).await.expect("tcp listener channel should work");
                            },
                            Err(err) => {
                                log::info!("[ProxyTcpListener] determine destination for {remote} error {err}");
                            },
                        }
                    });
                },
                out = self.rx.recv() => {
                    break Ok(out.expect("tcp listener channel should work"))
                }
            }
        }
    }
}
