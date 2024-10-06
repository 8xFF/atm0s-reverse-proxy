use std::{collections::HashSet, sync::Arc, time::Duration};

use parking_lot::Mutex;
use test_log::test;

use super::create_random_node;

#[test(tokio::test)]
async fn discovery_remain_node() {
    let (mut node1, addr1) = create_random_node(true).await;
    log::info!("created node1 {addr1}");
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });

    let (mut node2, addr2) = create_random_node(false).await;
    log::info!("created node2 {addr2}");
    let node2_requester = node2.requester();
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });

    let (mut node3, addr3) = create_random_node(false).await;
    log::info!("created node3 {addr3}");
    let node3_requester = node3.requester();
    let node3_neighbours = Arc::new(Mutex::new(HashSet::new()));
    let node3_neighbours_c = node3_neighbours.clone();
    tokio::spawn(async move {
        while let Ok(event) = node3.recv().await {
            match event {
                crate::P2pNetworkEvent::PeerConnected(peer_address) => {
                    node3_neighbours_c.lock().insert(peer_address);
                }
                crate::P2pNetworkEvent::PeerDisconnected(peer_address) => {
                    node3_neighbours_c.lock().remove(&peer_address);
                }
                crate::P2pNetworkEvent::Continue => {}
            }
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    node2_requester.connect(addr1).await.expect("should connect success");
    node3_requester.connect(addr2).await.expect("should connect success");

    tokio::time::sleep(Duration::from_secs(8)).await;

    // after some cycle node3 should have node1 as neighbour
    assert_eq!(node3_neighbours.lock().len(), 2);
}
