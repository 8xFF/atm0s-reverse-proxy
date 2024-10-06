use std::time::Duration;

use test_log::test;

use crate::alias_service::{AliasFoundLocation, AliasId, AliasService};

use super::create_random_node;

#[test(tokio::test)]
async fn alias_guard() {
    let (mut node1, _addr1) = create_random_node(true).await;
    let mut service1 = AliasService::new(node1.create_service(0.into()));
    let service1_requester = service1.requester();
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service1.recv().await {} });

    // we register alias before connect
    let alias_id: AliasId = 1000.into();
    let alia_guard = service1_requester.register(alias_id);

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(service1_requester.find(alias_id).await, Some(AliasFoundLocation::Local));
    drop(alia_guard);

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(service1_requester.find(alias_id).await, None);
}

#[test(tokio::test)]
async fn alias_multi_guards() {
    let (mut node1, _addr1) = create_random_node(true).await;
    let mut service1 = AliasService::new(node1.create_service(0.into()));
    let service1_requester = service1.requester();
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service1.recv().await {} });

    // we register alias before connect
    let alias_id: AliasId = 1000.into();
    let alia_guard1 = service1_requester.register(alias_id);
    let alia_guard2 = service1_requester.register(alias_id);

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(service1_requester.find(alias_id).await, Some(AliasFoundLocation::Local));
    drop(alia_guard1);

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(service1_requester.find(alias_id).await, Some(AliasFoundLocation::Local));
    drop(alia_guard2);

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(service1_requester.find(alias_id).await, None);
}

#[test(tokio::test)]
async fn alias_scan() {
    let (mut node1, addr1) = create_random_node(true).await;
    let mut service1 = AliasService::new(node1.create_service(0.into()));
    let service1_requester = service1.requester();
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service1.recv().await {} });

    let (mut node2, _addr2) = create_random_node(false).await;
    let node2_requester = node2.requester();
    let mut service2 = AliasService::new(node2.create_service(0.into()));
    let service2_requester = service2.requester();
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service2.recv().await {} });

    // we register alias before connect
    let alias_id: AliasId = 1000.into();
    let _alia_guard = service1_requester.register(alias_id);

    node2_requester.connect(addr1).await.expect("should connect success");

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(service2_requester.find(alias_id).await, Some(AliasFoundLocation::Scan(addr1)));
}

#[test(tokio::test)]
async fn alias_hint() {
    let (mut node1, addr1) = create_random_node(true).await;
    let mut service1 = AliasService::new(node1.create_service(0.into()));
    let service1_requester = service1.requester();
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service1.recv().await {} });

    let (mut node2, _addr2) = create_random_node(false).await;
    let node2_requester = node2.requester();
    let mut service2 = AliasService::new(node2.create_service(0.into()));
    let service2_requester = service2.requester();
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service2.recv().await {} });

    node2_requester.connect(addr1).await.expect("should connect success");

    tokio::time::sleep(Duration::from_secs(1)).await;

    // we register alias after connect
    let alias_id: AliasId = 1000.into();
    let _alia_guard = service1_requester.register(alias_id);

    tokio::time::sleep(Duration::from_secs(1)).await;

    assert_eq!(service2_requester.find(alias_id).await, Some(AliasFoundLocation::Hint(addr1)));
}

#[test(tokio::test)]
async fn alias_timeout() {
    let (mut node1, addr1) = create_random_node(true).await;
    let mut service1 = AliasService::new(node1.create_service(0.into()));
    tokio::spawn(async move { while let Ok(_) = node1.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service1.recv().await {} });

    let (mut node2, _addr2) = create_random_node(false).await;
    let node2_requester = node2.requester();
    let mut service2 = AliasService::new(node2.create_service(0.into()));
    let service2_requester = service2.requester();
    tokio::spawn(async move { while let Ok(_) = node2.recv().await {} });
    tokio::spawn(async move { while let Ok(_) = service2.recv().await {} });

    node2_requester.connect(addr1).await.expect("should connect success");

    tokio::time::sleep(Duration::from_secs(1)).await;

    let alias_id: AliasId = 1000.into();
    assert_eq!(service2_requester.find(alias_id).await, None);
}
