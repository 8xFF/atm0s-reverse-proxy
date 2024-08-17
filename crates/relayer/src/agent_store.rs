use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use async_std::channel::Sender;

use crate::ProxyTunnel;

#[derive(Clone, Default)]
pub struct AgentStore {
    #[allow(clippy::type_complexity)]
    agents: Arc<RwLock<HashMap<u64, Sender<Box<dyn ProxyTunnel>>>>>,
}

impl AgentStore {
    pub fn add(&self, id: u64, tx: Sender<Box<dyn ProxyTunnel>>) {
        self.agents
            .write()
            .expect("Should write agents")
            .insert(id, tx);
    }

    pub fn get(&self, id: u64) -> Option<Sender<Box<dyn ProxyTunnel>>> {
        self.agents
            .read()
            .expect("Should write agents")
            .get(&id)
            .cloned()
    }

    pub fn remove(&self, id: u64) {
        self.agents
            .write()
            .expect("Should write agents")
            .remove(&id);
    }
}
