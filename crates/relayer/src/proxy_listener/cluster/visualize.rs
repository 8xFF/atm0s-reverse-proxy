use std::net::SocketAddr;

use async_std::channel::{Receiver, Sender};
use atm0s_sdn::{services::visualization::ConnectionInfo, NodeId};
use serde::Serialize;

use super::NodeInfo;

#[derive(Debug, Clone, Serialize)]
pub struct ConnInfo {
    uuid: u64,
    outgoing: bool,
    dest: NodeId,
    remote: SocketAddr,
    rtt_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum NetworkVisualizeEvent {
    Snapshot(Vec<(NodeId, NodeInfo, Vec<ConnInfo>)>),
    NodeChanged(NodeId, NodeInfo, Vec<ConnInfo>),
    NodeRemoved(NodeId),
}

pub struct NetworkVisualizeSdk {
    sender: Sender<NetworkVisualizeEvent>,
}

impl NetworkVisualizeSdk {
    pub async fn snapshot(&self, nodes: Vec<(NodeId, NodeInfo, Vec<ConnectionInfo>)>) {
        let mut snapshot = Vec::new();
        for (node_id, node_info, conns) in nodes {
            let conns: Vec<ConnInfo> = conns
                .into_iter()
                .map(|c| ConnInfo {
                    uuid: c.conn.session(),
                    outgoing: c.conn.is_outgoing(),
                    dest: c.dest,
                    remote: c.remote,
                    rtt_ms: c.rtt_ms,
                })
                .collect();
            snapshot.push((node_id, node_info, conns));
        }
        let _ = self
            .sender
            .send(NetworkVisualizeEvent::Snapshot(snapshot))
            .await;
    }

    pub async fn node_change(&self, node: (NodeId, NodeInfo, Vec<ConnectionInfo>)) {
        let (node_id, node_info, conns) = node;
        let conns: Vec<ConnInfo> = conns
            .into_iter()
            .map(|c| ConnInfo {
                uuid: c.conn.session(),
                outgoing: c.conn.is_outgoing(),
                dest: c.dest,
                remote: c.remote,
                rtt_ms: c.rtt_ms,
            })
            .collect();
        let _ = self
            .sender
            .send(NetworkVisualizeEvent::NodeChanged(
                node_id, node_info, conns,
            ))
            .await;
    }

    pub async fn node_removed(&self, node: NodeId) {
        let _ = self
            .sender
            .send(NetworkVisualizeEvent::NodeRemoved(node))
            .await;
    }
}

pub struct NetworkVisualize {
    rx: Receiver<NetworkVisualizeEvent>,
}

impl NetworkVisualize {
    pub fn new() -> (Self, NetworkVisualizeSdk) {
        let (tx, rx) = async_std::channel::bounded(100);
        (Self { rx }, NetworkVisualizeSdk { sender: tx })
    }

    pub async fn pop_request(&mut self) -> Option<NetworkVisualizeEvent> {
        loop {
            match self.rx.recv().await {
                Ok(event) => return Some(event),
                Err(_) => return None,
            }
        }
    }
}
