use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

use anyhow::anyhow;
use derive_more::derive::{Display, From};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use tokio::{
    select,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::Interval,
};

use crate::{
    stream::P2pQuicStream,
    utils::{now_ms, ErrorExt, ErrorExt2},
    PeerAddress,
};

use super::{P2pService, P2pServiceEvent, P2pServiceRequester};

const LRU_CACHE_SIZE: usize = 1_000_000;
const HINT_TIMEOUT_MS: u64 = 500;
const SCAN_TIMEOUT_MS: u64 = 1000;

#[derive(Debug, From, Display, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy)]
pub struct AliasId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasFoundLocation {
    Local,
    Hint(PeerAddress),
    Scan(PeerAddress),
}

pub enum AliasStreamLocation {
    Local,
    Hint(P2pQuicStream),
    Scan(P2pQuicStream),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum AliasMessage {
    NotifySet(AliasId),
    NotifyDel(AliasId),
    Check(AliasId),
    Scan(AliasId),
    Found(AliasId),
    NotFound(AliasId),
    // when a node
    Shutdown,
}

enum AliasControl {
    Register(AliasId),
    Unregister(AliasId),
    Find(AliasId, oneshot::Sender<Option<AliasFoundLocation>>),
    Shutdown,
}

#[derive(Debug)]
pub struct AliasGuard {
    alias: AliasId,
    tx: UnboundedSender<AliasControl>,
}

impl Drop for AliasGuard {
    fn drop(&mut self) {
        log::info!("[AliasGuard] drop {} => auto unregister", self.alias);
        self.tx.send(AliasControl::Unregister(self.alias)).expect("alias service main channal should work");
    }
}

#[derive(Debug, Clone)]
pub struct AliasServiceRequester {
    tx: UnboundedSender<AliasControl>,
}

impl AliasServiceRequester {
    pub fn register<A: Into<AliasId>>(&self, alias: A) -> AliasGuard {
        let alias: AliasId = alias.into();
        log::info!("[AliasServiceRequester] register alias {alias}");
        self.tx.send(AliasControl::Register(alias)).expect("alias service main channal should work");

        AliasGuard { alias, tx: self.tx.clone() }
    }

    pub async fn find<A: Into<AliasId>>(&self, alias: A) -> Option<AliasFoundLocation> {
        let alias: AliasId = alias.into();
        log::debug!("[AliasServiceRequester] find alias {alias}");
        let (tx, rx) = oneshot::channel();
        self.tx.send(AliasControl::Find(alias, tx)).expect("alias service main channal should work");
        rx.await.ok()?
    }

    pub async fn open_stream<A: Into<AliasId>>(&self, alias: A, over_service: P2pServiceRequester, meta: Vec<u8>) -> anyhow::Result<AliasStreamLocation> {
        match self.find(alias).await {
            Some(AliasFoundLocation::Local) => Ok(AliasStreamLocation::Local),
            Some(AliasFoundLocation::Hint(dest)) => over_service.open_stream(dest, meta).await.map(AliasStreamLocation::Hint),
            Some(AliasFoundLocation::Scan(dest)) => over_service.open_stream(dest, meta).await.map(AliasStreamLocation::Scan),
            None => Err(anyhow!("alias not found")),
        }
    }

    pub fn shutdown(&self) {
        log::info!("[AliasServiceRequester] shutdown");
        self.tx.send(AliasControl::Shutdown).expect("alias service main channal should work");
    }
}

enum FindRequestState {
    CheckHint(u64, HashSet<PeerAddress>),
    Scan(u64),
}

struct FindRequest {
    state: FindRequestState,
    waits: Vec<oneshot::Sender<Option<AliasFoundLocation>>>,
}

#[derive(Debug, PartialEq, Eq)]
enum InternalOutput {
    Broadcast(AliasMessage),
    Unicast(PeerAddress, AliasMessage),
}

struct AliasServiceInternal {
    local: HashMap<AliasId, u8>,
    cache: LruCache<AliasId, HashSet<PeerAddress>>,
    find_reqs: HashMap<AliasId, FindRequest>,
    outs: VecDeque<InternalOutput>,
}

pub struct AliasService {
    service: P2pService,
    tx: UnboundedSender<AliasControl>,
    rx: UnboundedReceiver<AliasControl>,
    internal: AliasServiceInternal,
    interval: Interval,
}

impl AliasService {
    pub fn new(service: P2pService) -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            service,
            tx,
            rx,
            internal: AliasServiceInternal {
                cache: LruCache::new(LRU_CACHE_SIZE.try_into().expect("")),
                find_reqs: HashMap::new(),
                outs: VecDeque::new(),
                local: HashMap::new(),
            },
            interval: tokio::time::interval(Duration::from_secs(1)),
        }
    }

    pub fn requester(&self) -> AliasServiceRequester {
        AliasServiceRequester { tx: self.tx.clone() }
    }

    pub async fn recv(&mut self) -> anyhow::Result<()> {
        select! {
            _ = self.interval.tick() => {
                self.on_tick().await;
                Ok(())
            },
            event = self.service.recv() => match event.expect("service channel should work") {
                P2pServiceEvent::Unicast(from, data) => {
                    if let Ok(msg) = bincode::deserialize::<AliasMessage>(&data) {
                        self.on_msg(from, msg).await;
                    }
                    Ok(())
                }
                P2pServiceEvent::Broadcast(from, data) => {
                    if let Ok(msg) = bincode::deserialize::<AliasMessage>(&data) {
                        self.on_msg(from, msg).await;
                    }
                    Ok(())
                }
                P2pServiceEvent::Stream(..) => Ok(()),
            },
            control = self.rx.recv() => {
                self.on_control(control.expect("service channel should work")).await;
                Ok(())
            }
        }
    }

    async fn on_tick(&mut self) {
        self.internal.on_tick(now_ms());
        self.pop_internal().await;
    }

    async fn on_msg(&mut self, from: PeerAddress, msg: AliasMessage) {
        log::debug!("[AliasService] on msg from {from}, {msg:?}");
        self.internal.on_msg(now_ms(), from, msg);
        self.pop_internal().await;
    }

    async fn on_control(&mut self, control: AliasControl) {
        self.internal.on_control(now_ms(), control);
        self.pop_internal().await;
    }

    async fn pop_internal(&mut self) {
        while let Some(out) = self.internal.pop_output() {
            match out {
                InternalOutput::Broadcast(msg) => {
                    self.service.send_broadcast(bincode::serialize(&msg).expect("should serialie")).await;
                }
                InternalOutput::Unicast(dest, msg) => {
                    self.service
                        .send_unicast(dest, bincode::serialize(&msg).expect("should serialie"))
                        .await
                        .print_on_err("[AliasService] send unicast");
                }
            }
        }
    }
}

impl AliasServiceInternal {
    fn on_tick(&mut self, now: u64) {
        let mut timeout_reqs = vec![];
        for (alias_id, req) in self.find_reqs.iter_mut() {
            match req.state {
                FindRequestState::CheckHint(requested_at, ref mut _hash_set) => {
                    if requested_at + HINT_TIMEOUT_MS <= now {
                        req.state = FindRequestState::Scan(now);
                    }
                }
                FindRequestState::Scan(requested_at) => {
                    if requested_at + SCAN_TIMEOUT_MS <= now {
                        timeout_reqs.push(*alias_id);
                        while let Some(tx) = req.waits.pop() {
                            tx.send(None).print_on_err2("");
                        }
                    }
                }
            }
        }

        for alias_id in timeout_reqs {
            self.find_reqs.remove(&alias_id);
        }
    }

    fn on_msg(&mut self, now: u64, from: PeerAddress, msg: AliasMessage) {
        match msg {
            AliasMessage::NotifySet(alias_id) => {
                let slot = self.cache.get_or_insert_mut(alias_id, || HashSet::new());
                slot.insert(from);
            }
            AliasMessage::NotifyDel(alias_id) => {
                if let Some(slot) = self.cache.get_mut(&alias_id) {
                    slot.remove(&from);
                    if slot.is_empty() {
                        self.cache.pop(&alias_id);
                    }
                }
            }
            AliasMessage::Check(alias_id) => {
                if self.local.contains_key(&alias_id) {
                    self.outs.push_back(InternalOutput::Unicast(from, AliasMessage::Found(alias_id)));
                } else {
                    self.outs.push_back(InternalOutput::Unicast(from, AliasMessage::NotFound(alias_id)));
                }
            }
            AliasMessage::Scan(alias_id) => {
                if self.local.contains_key(&alias_id) {
                    self.outs.push_back(InternalOutput::Unicast(from, AliasMessage::Found(alias_id)));
                }
            }
            AliasMessage::Found(alias_id) => {
                let slot = self.cache.get_or_insert_mut(alias_id, || HashSet::new());
                slot.insert(from);

                if let Some(req) = self.find_reqs.remove(&alias_id) {
                    let found = if matches!(req.state, FindRequestState::Scan(_)) {
                        AliasFoundLocation::Scan(from)
                    } else {
                        AliasFoundLocation::Hint(from)
                    };
                    for tx in req.waits {
                        tx.send(Some(found)).print_on_err2("[AliasServiceInternal] send query response");
                    }
                }
            }
            AliasMessage::NotFound(alias_id) => {
                if let Some(slot) = self.cache.get_mut(&alias_id) {
                    slot.remove(&from);
                    if slot.is_empty() {
                        self.cache.pop(&alias_id);
                    }
                }

                if let Some(req) = self.find_reqs.get_mut(&alias_id) {
                    match req.state {
                        FindRequestState::CheckHint(_, ref mut hint_peers) => {
                            hint_peers.remove(&from);
                            if hint_peers.is_empty() {
                                //not found => should switch to scan
                                req.state = FindRequestState::Scan(now);
                                self.outs.push_back(InternalOutput::Broadcast(AliasMessage::Scan(alias_id)));
                            }
                        }
                        FindRequestState::Scan(_) => {}
                    }
                }
            }
            AliasMessage::Shutdown => {
                let mut removed_alias_ids = vec![];
                for (k, _v) in &mut self.cache {
                    removed_alias_ids.push(*k);
                }
                for alias_id in removed_alias_ids {
                    self.cache.pop(&alias_id);
                }
            }
        }
    }

    fn on_control(&mut self, now: u64, control: AliasControl) {
        match control {
            AliasControl::Register(alias_id) => {
                let ref_count = self.local.entry(alias_id).or_default();
                *ref_count += 1;
                self.outs.push_back(InternalOutput::Broadcast(AliasMessage::NotifySet(alias_id)));
            }
            AliasControl::Unregister(alias_id) => {
                if let Some(ref_count) = self.local.get_mut(&alias_id) {
                    *ref_count -= 1;
                    if *ref_count == 0 {
                        self.local.remove(&alias_id);
                        self.outs.push_back(InternalOutput::Broadcast(AliasMessage::NotifyDel(alias_id)));
                    }
                }
            }
            AliasControl::Find(alias_id, sender) => {
                if let Some(req) = self.find_reqs.get_mut(&alias_id) {
                    req.waits.push(sender);
                    return;
                }

                if self.local.contains_key(&alias_id) {
                    sender.send(Some(AliasFoundLocation::Local)).print_on_err2("[AliasServiceInternal] send query response");
                } else if let Some(slot) = self.cache.get(&alias_id) {
                    for peer in slot {
                        self.outs.push_back(InternalOutput::Unicast(*peer, AliasMessage::Check(alias_id)));
                    }
                    self.find_reqs.insert(
                        alias_id,
                        FindRequest {
                            state: FindRequestState::CheckHint(now, slot.clone()),
                            waits: vec![sender],
                        },
                    );
                } else {
                    self.outs.push_back(InternalOutput::Broadcast(AliasMessage::Scan(alias_id)));
                    self.find_reqs.insert(
                        alias_id,
                        FindRequest {
                            state: FindRequestState::Scan(now),
                            waits: vec![sender],
                        },
                    );
                }
            }
            AliasControl::Shutdown => {
                self.outs.push_back(InternalOutput::Broadcast(AliasMessage::Shutdown));
            }
        }
    }

    fn pop_output(&mut self) -> Option<InternalOutput> {
        self.outs.pop_front()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct TestContext {
        internal: AliasServiceInternal,
        now: u64,
    }

    impl TestContext {
        fn new() -> Self {
            Self {
                internal: AliasServiceInternal {
                    local: HashMap::new(),
                    cache: LruCache::new(LRU_CACHE_SIZE.try_into().unwrap()),
                    find_reqs: HashMap::new(),
                    outs: VecDeque::new(),
                },
                now: 1000,
            }
        }

        fn advance_time(&mut self, ms: u64) {
            self.now += ms;
        }

        fn collect_outputs(&mut self) -> Vec<InternalOutput> {
            let mut outputs = Vec::new();
            while let Some(output) = self.internal.pop_output() {
                outputs.push(output);
            }
            outputs
        }
    }

    #[test]
    fn test_register_alias() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);

        // Test registering an alias
        ctx.internal.on_control(ctx.now, AliasControl::Register(alias_id));

        // Verify local set contains the alias
        assert!(ctx.internal.local.contains_key(&alias_id));

        // Verify broadcast message
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            InternalOutput::Broadcast(AliasMessage::NotifySet(id)) => assert_eq!(*id, alias_id),
            _ => panic!("Expected broadcast NotifySet message"),
        }
    }

    #[test]
    fn test_unregister_alias() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);

        // Register first
        ctx.internal.on_control(ctx.now, AliasControl::Register(alias_id));
        ctx.collect_outputs(); // Clear outputs

        // Test unregistering
        ctx.internal.on_control(ctx.now, AliasControl::Unregister(alias_id));

        // Verify local set doesn't contain the alias
        assert!(!ctx.internal.local.contains_key(&alias_id));

        // Verify broadcast message
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            InternalOutput::Broadcast(AliasMessage::NotifyDel(id)) => assert_eq!(*id, alias_id),
            _ => panic!("Expected broadcast NotifyDel message"),
        }
    }

    #[test]
    fn test_find_local_alias() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);

        // Register alias locally
        ctx.internal.on_control(ctx.now, AliasControl::Register(alias_id));
        ctx.collect_outputs(); // Clear outputs

        // Create a oneshot channel for the find response
        let (tx, mut rx) = oneshot::channel();

        // Test finding the local alias
        ctx.internal.on_control(ctx.now, AliasControl::Find(alias_id, tx));

        // Verify response
        let response = rx.try_recv().expect("Should have a response");
        assert_eq!(response, Some(AliasFoundLocation::Local));

        // Verify no outputs (shouldn't need to broadcast for local find)
        let outputs = ctx.collect_outputs();
        assert!(outputs.is_empty());
    }

    #[test]
    fn test_find_cached_alias_found() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);
        let peer_addr = PeerAddress("127.0.0.1:1".parse().expect(""));

        // Add alias to cache
        ctx.internal.on_msg(ctx.now, peer_addr, AliasMessage::NotifySet(alias_id));

        // Create a oneshot channel for the find response
        let (tx, mut rx) = oneshot::channel();

        // Test finding the cached alias
        ctx.internal.on_control(ctx.now, AliasControl::Find(alias_id, tx));

        // Verify unicast message to check with cached peer
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs, vec![InternalOutput::Unicast(peer_addr, AliasMessage::Check(alias_id))]);

        // Simulate peer response
        ctx.internal.on_msg(ctx.now, peer_addr, AliasMessage::Found(alias_id));

        // Verify find response
        let response = rx.try_recv().expect("Should have a response");
        assert_eq!(response, Some(AliasFoundLocation::Hint(peer_addr)));
    }

    #[test]
    fn test_find_cached_alias_not_found() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);
        let peer_addr = PeerAddress("127.0.0.1:1".parse().expect(""));

        // Add alias to cache
        ctx.internal.on_msg(ctx.now, peer_addr, AliasMessage::NotifySet(alias_id));

        // Create a oneshot channel for the find response
        let (tx, _rx) = oneshot::channel();

        // Test finding the cached alias
        ctx.internal.on_control(ctx.now, AliasControl::Find(alias_id, tx));

        // Verify unicast message to check with cached peer
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs, vec![InternalOutput::Unicast(peer_addr, AliasMessage::Check(alias_id))]);

        // Simulate peer response
        ctx.internal.on_msg(ctx.now, peer_addr, AliasMessage::NotFound(alias_id));

        // Verify broadcast scan message
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs, vec![InternalOutput::Broadcast(AliasMessage::Scan(alias_id))]);
    }

    #[test]
    fn test_find_timeout() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);

        // Create a oneshot channel for the find response
        let (tx, mut rx) = oneshot::channel();

        // Test finding a non-existent alias
        ctx.internal.on_control(ctx.now, AliasControl::Find(alias_id, tx));

        // Verify broadcast scan message
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs, vec![InternalOutput::Broadcast(AliasMessage::Scan(alias_id))]);

        // Advance time past timeout
        ctx.advance_time(SCAN_TIMEOUT_MS + 1);
        ctx.internal.on_tick(ctx.now);

        // Verify timeout response
        let response = rx.try_recv().expect("Should have a response");
        assert_eq!(response, None);
    }

    #[test]
    fn test_shutdown() {
        let mut ctx = TestContext::new();
        let alias_id = AliasId(1);
        let peer_addr = PeerAddress("127.0.0.1:1".parse().expect(""));

        // Add some data to cache
        let mut peers = HashSet::new();
        peers.insert(peer_addr);
        ctx.internal.cache.put(alias_id, peers);

        // Test shutdown
        ctx.internal.on_control(ctx.now, AliasControl::Shutdown);

        // Verify broadcast shutdown message
        let outputs = ctx.collect_outputs();
        assert_eq!(outputs, vec![InternalOutput::Broadcast(AliasMessage::Shutdown)]);

        // Simulate receiving shutdown message
        ctx.internal.on_msg(ctx.now, peer_addr, AliasMessage::Shutdown);

        // Verify cache is cleared
        assert!(ctx.internal.cache.is_empty());
    }
}
