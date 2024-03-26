use std::collections::HashMap;

use async_std::channel::{Receiver, Sender};
use atm0s_sdn::NodeId;

enum AliasSdkEvent {
    Register(u64),
    Unregister(u64),
    Query(u64, Sender<Option<NodeId>>),
}

#[derive(Clone)]
pub struct AliasSdk {
    requester: Sender<AliasSdkEvent>,
}

impl AliasSdk {
    pub async fn find_alias(&self, alias: u64) -> Option<NodeId> {
        log::info!("Querying alias: {}", alias);
        let (tx, rx) = async_std::channel::bounded(1);
        self.requester
            .send(AliasSdkEvent::Query(alias, tx))
            .await
            .ok()?;
        rx.recv().await.ok().flatten()
    }

    pub async fn register_alias(&self, alias: u64) {
        if let Err(e) = self.requester.send(AliasSdkEvent::Register(alias)).await {
            log::error!("Failed to register alias: {:?}", e);
        }
    }

    pub async fn unregister_alias(&self, alias: u64) {
        if let Err(e) = self.requester.send(AliasSdkEvent::Unregister(alias)).await {
            log::error!("Failed to unregister alias: {:?}", e);
        }
    }
}

pub enum AliasAsyncEvent {
    Register(u64),
    Unregister(u64),
    Query(u64),
}

pub struct AliasAsync {
    rx: Receiver<AliasSdkEvent>,
    waits: HashMap<u64, Vec<Sender<Option<NodeId>>>>,
}

impl AliasAsync {
    pub fn new() -> (Self, AliasSdk) {
        let (tx, rx) = async_std::channel::bounded(100);
        (
            Self {
                rx,
                waits: HashMap::new(),
            },
            AliasSdk { requester: tx },
        )
    }

    pub fn push_response(&mut self, alias: u64, node_id: Option<NodeId>) {
        if let Some(tx) = self.waits.remove(&alias) {
            log::info!("Find alias {} response: {:?}", alias, node_id);
            for tx in tx {
                tx.try_send(node_id).ok();
            }
        }
    }

    pub fn pop_request(&mut self) -> Option<AliasAsyncEvent> {
        loop {
            match self.rx.try_recv().ok()? {
                AliasSdkEvent::Query(alias, tx) => {
                    if let Some(req) = self.waits.get_mut(&alias) {
                        req.push(tx);
                    } else {
                        self.waits.insert(alias, vec![tx]);
                        return Some(AliasAsyncEvent::Query(alias));
                    }
                }
                AliasSdkEvent::Register(alias) => {
                    return Some(AliasAsyncEvent::Register(alias));
                }
                AliasSdkEvent::Unregister(alias) => {
                    return Some(AliasAsyncEvent::Unregister(alias));
                }
            }
        }
    }
}
