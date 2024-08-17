use std::{
    fmt::Debug,
    io::IoSliceMut,
    net::{SocketAddr, SocketAddrV4},
    ops::DerefMut,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use async_std::channel::{Receiver, Sender};
use futures::StreamExt;
use quinn::{
    udp::{EcnCodepoint, RecvMeta, Transmit},
    AsyncUdpSocket, UdpPoller,
};

use super::vnet::{NetworkPkt, OutEvent};

#[derive(Debug)]
pub struct Poller {}

impl UdpPoller for Poller {
    fn poll_writable(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<std::io::Result<()>> {
        //TODO implement this for better performace
        Poll::Ready(Ok(()))
    }
}

pub struct VirtualUdpSocket {
    port: u16,
    addr: SocketAddr,
    rx: Mutex<Receiver<NetworkPkt>>,
    tx: Sender<OutEvent>,
    close_socket_tx: Sender<u16>,
}

impl VirtualUdpSocket {
    pub fn new(
        node_id: u32,
        port: u16,
        tx: Sender<OutEvent>,
        rx: Receiver<NetworkPkt>,
        close_socket_tx: Sender<u16>,
    ) -> Self {
        Self {
            port,
            addr: SocketAddr::V4(SocketAddrV4::new(node_id.into(), port)),
            rx: Mutex::new(rx),
            tx,
            close_socket_tx,
        }
    }
}

impl Debug for VirtualUdpSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualUdpSocket").finish()
    }
}

impl AsyncUdpSocket for VirtualUdpSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::into_pin(Box::new(Poller {}))
    }

    fn try_send(&self, transmit: &Transmit) -> std::io::Result<()> {
        match transmit.destination {
            SocketAddr::V4(addr) => {
                let pkt = NetworkPkt {
                    local_port: self.port,
                    remote: u32::from_be_bytes(addr.ip().octets()),
                    remote_port: addr.port(),
                    data: transmit.contents.to_vec().into(),
                    meta: transmit.ecn.map(|x| x as u8).unwrap_or(0),
                };
                log::debug!("{} sending {} bytes to {}", self.addr, pkt.data.len(), addr);
                if !self.tx.is_full() && self.tx.try_send(OutEvent::Pkt(pkt)).is_ok() {
                    Ok(())
                } else {
                    //Err(std::io::ErrorKind::WouldBlock.into())
                    //TODO avoid fake send success, need to implement awake mechanism
                    Ok(())
                }
            }
            _ => Err(std::io::ErrorKind::ConnectionRefused.into()),
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<std::io::Result<usize>> {
        let mut rx = self.rx.lock().expect("Should lock mutex");
        match rx.poll_next_unpin(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(pkt)) => {
                let len = pkt.data.len();
                if len <= bufs[0].len() {
                    let addr =
                        SocketAddr::V4(SocketAddrV4::new(pkt.remote.into(), pkt.remote_port));
                    log::debug!("{} received {} bytes from {}", self.addr, len, addr);
                    bufs[0].deref_mut()[0..len].copy_from_slice(&pkt.data);
                    meta[0] = quinn::udp::RecvMeta {
                        addr,
                        len,
                        stride: len,
                        ecn: if pkt.meta == 0 {
                            None
                        } else {
                            EcnCodepoint::from_bits(pkt.meta)
                        },
                        dst_ip: None,
                    };
                    std::task::Poll::Ready(Ok(1))
                } else {
                    log::warn!(
                        "Buffer too small for packet {} vs {}, dropping",
                        len,
                        bufs[0].len()
                    );
                    std::task::Poll::Pending
                }
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Socket closed",
            ))),
        }
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

impl Drop for VirtualUdpSocket {
    fn drop(&mut self) {
        if let Err(e) = self.close_socket_tx.try_send(self.port) {
            log::error!("Failed to send close socket: {:?}", e);
        }
    }
}
