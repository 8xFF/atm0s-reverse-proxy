use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use async_std::channel::Sender;

use crate::ProxyTunnel;

pub struct AgentEntry {
    pub session: u64,
    pub tx: Sender<Box<dyn ProxyTunnel>>,
}

#[derive(Clone, Default)]
pub struct AgentStore {
    #[allow(clippy::type_complexity)]
    agents: Arc<RwLock<HashMap<u64, AgentEntry>>>,
}

impl AgentStore {
    pub fn add(&self, id: u64, session: u64, tx: Sender<Box<dyn ProxyTunnel>>) {
        self.agents
            .write()
            .expect("Should write agents")
            .insert(id, AgentEntry { tx, session });
    }

    pub fn get(&self, id: u64) -> Option<Sender<Box<dyn ProxyTunnel>>> {
        self.agents
            .read()
            .expect("Should write agents")
            .get(&id)
            .map(|entry| entry.tx.clone())
    }

    pub fn remove(&self, id: u64, session: u64) -> bool {
        let mut storage = self.agents.write().expect("Should write agents");

        let current = storage.get(&id);
        if let Some(entry) = current {
            if entry.session == session {
                storage.remove(&id);
                return true;
            }
        }

        false
    }
}
