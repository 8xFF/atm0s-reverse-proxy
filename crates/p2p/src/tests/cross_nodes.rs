use std::time::Duration;

use test_log::test;

use crate::P2pServiceEvent;

use super::create_random_node;

#[test(tokio::test)]
async fn send_direct() {
    let (mut node1, addr1) = create_random_node(true).await;
    let mut service1 = node1.create_service(0.into());
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });

    let (mut node2, addr2) = create_random_node(false).await;
    let node2_requester = node2.requester();
    let mut service2 = node2.create_service(0.into());
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });

    node2_requester.connect(addr1).await.expect("should connect success");

    tokio::time::sleep(Duration::from_secs(1)).await;

    let data = "from_node1".as_bytes().to_vec();
    service1.send_unicast(addr2, data.clone()).await.expect("should send ok");
    assert_eq!(service2.recv().await, Some(P2pServiceEvent::Unicast(addr1, data)));

    let data = "from_node2".as_bytes().to_vec();
    service2.send_unicast(addr1, data.clone()).await.expect("should send ok");
    assert_eq!(service1.recv().await, Some(P2pServiceEvent::Unicast(addr2, data)));
}

#[test(tokio::test)]
async fn send_error() {
    // without connect 2 peers, it should error to send data
    let (mut node1, addr1) = create_random_node(true).await;
    let service1 = node1.create_service(0.into());
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });

    let (mut node2, addr2) = create_random_node(false).await;
    let service2 = node2.create_service(0.into());
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });

    let data = "from_node1".as_bytes().to_vec();
    assert!(service1.send_unicast(addr2, data.clone()).await.is_err());

    let data = "from_node2".as_bytes().to_vec();
    assert!(service2.send_unicast(addr1, data.clone()).await.is_err());
}

#[test(tokio::test)]
async fn send_relay() {
    let (mut node1, addr1) = create_random_node(false).await;
    let mut service1 = node1.create_service(0.into());
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });

    let (mut node2, addr2) = create_random_node(false).await;
    let node2_requester = node2.requester();
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });

    let (mut node3, addr3) = create_random_node(false).await;
    let node3_requester = node3.requester();
    let mut service3 = node3.create_service(0.into());
    tokio::spawn(async move { while let Ok(_) = node3.recv().await {} });

    node2_requester.connect(addr1).await.expect("should connect success");
    node3_requester.connect(addr2).await.expect("should connect success");

    tokio::time::sleep(Duration::from_secs(1)).await;

    let data = "from_node1".as_bytes().to_vec();
    service1.send_unicast(addr3, data.clone()).await.expect("should send ok");
    assert_eq!(service3.recv().await, Some(P2pServiceEvent::Unicast(addr1, data)));

    let data = "from_node3".as_bytes().to_vec();
    service3.send_unicast(addr1, data.clone()).await.expect("should send ok");
    assert_eq!(service1.recv().await, Some(P2pServiceEvent::Unicast(addr3, data)));
}
