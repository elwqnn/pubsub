mod helpers;

use bytes::Bytes;
use tokio::time::{timeout, Duration};

use topiq_client::{Client, ConnectOptions};

use crate::helpers::start_test_server;

#[tokio::test]
async fn basic_pub_sub() {
    let (addr, shutdown) = start_test_server().await;

    let client = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let mut sub = client.subscribe("test.topic").await.unwrap();

    // Small delay to let subscription propagate.
    tokio::time::sleep(Duration::from_millis(50)).await;

    client
        .publish("test.topic", Bytes::from("hello world"))
        .await
        .unwrap();

    let msg = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .expect("timed out waiting for message")
        .expect("subscription closed");

    assert_eq!(msg.topic.as_str(), "test.topic");
    assert_eq!(msg.payload, Bytes::from("hello world"));

    client.close().await;
    shutdown.cancel();
}

#[tokio::test]
async fn wildcard_star_subscription() {
    let (addr, shutdown) = start_test_server().await;

    let client = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let mut sub = client.subscribe("sensors.*.room1").await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    client
        .publish("sensors.temp.room1", Bytes::from("25"))
        .await
        .unwrap();
    client
        .publish("sensors.humidity.room1", Bytes::from("60"))
        .await
        .unwrap();
    // Should NOT match:
    client
        .publish("sensors.temp.room2", Bytes::from("30"))
        .await
        .unwrap();

    let msg1 = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .unwrap()
        .unwrap();
    let msg2 = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(msg1.payload, Bytes::from("25"));
    assert_eq!(msg2.payload, Bytes::from("60"));

    // No third message should arrive.
    let result = timeout(Duration::from_millis(200), sub.next_message()).await;
    assert!(result.is_err(), "should not receive non-matching message");

    client.close().await;
    shutdown.cancel();
}

#[tokio::test]
async fn wildcard_gt_subscription() {
    let (addr, shutdown) = start_test_server().await;

    let client = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let mut sub = client.subscribe("events.>").await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    client
        .publish("events.user.login", Bytes::from("alice"))
        .await
        .unwrap();
    client
        .publish("events.system", Bytes::from("startup"))
        .await
        .unwrap();

    let msg1 = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .unwrap()
        .unwrap();
    let msg2 = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(msg1.topic.as_str(), "events.user.login");
    assert_eq!(msg2.topic.as_str(), "events.system");

    client.close().await;
    shutdown.cancel();
}

#[tokio::test]
async fn multiple_subscribers_fanout() {
    let (addr, shutdown) = start_test_server().await;

    let client1 = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let client2 = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let publisher = Client::connect(ConnectOptions::new(addr)).await.unwrap();

    let mut sub1 = client1.subscribe("news").await.unwrap();
    let mut sub2 = client2.subscribe("news").await.unwrap();

    // Allow time for subscriptions across 3 connections to propagate.
    tokio::time::sleep(Duration::from_millis(150)).await;

    publisher
        .publish("news", Bytes::from("breaking"))
        .await
        .unwrap();

    let msg1 = timeout(Duration::from_secs(2), sub1.next_message())
        .await
        .unwrap()
        .unwrap();
    let msg2 = timeout(Duration::from_secs(2), sub2.next_message())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(msg1.payload, Bytes::from("breaking"));
    assert_eq!(msg2.payload, Bytes::from("breaking"));

    publisher.close().await;
    client1.close().await;
    client2.close().await;
    shutdown.cancel();
}

#[tokio::test]
async fn unsubscribe_stops_messages() {
    let (addr, shutdown) = start_test_server().await;

    let client = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let mut sub = client.subscribe("test").await.unwrap();
    let sid = sub.sid();

    tokio::time::sleep(Duration::from_millis(50)).await;

    client
        .publish("test", Bytes::from("before"))
        .await
        .unwrap();
    let msg = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.payload, Bytes::from("before"));

    // Unsubscribe and wait for server to process it.
    client.unsubscribe(sid).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    client
        .publish("test", Bytes::from("after"))
        .await
        .unwrap();

    // The subscription channel may still be open but should receive no new messages.
    let result = timeout(Duration::from_millis(500), sub.next_message()).await;
    assert!(
        result.is_err() || result.unwrap().is_none(),
        "should not receive after unsubscribe"
    );

    client.close().await;
    shutdown.cancel();
}

#[tokio::test]
async fn publish_with_reply_to() {
    let (addr, shutdown) = start_test_server().await;

    let client = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    let mut sub = client.subscribe("request").await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    client
        .publish_with_reply("request", Bytes::from("ping"), "reply.inbox")
        .await
        .unwrap();

    let msg = timeout(Duration::from_secs(2), sub.next_message())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(msg.topic.as_str(), "request");
    assert_eq!(msg.payload, Bytes::from("ping"));
    assert_eq!(msg.reply_to.unwrap().as_str(), "reply.inbox");

    client.close().await;
    shutdown.cancel();
}

#[tokio::test]
async fn connection_cleanup_on_disconnect() {
    let (addr, shutdown) = start_test_server().await;

    // Client subscribes, then disconnects.
    {
        let client = Client::connect(ConnectOptions::new(addr)).await.unwrap();
        let _sub = client.subscribe("ephemeral").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        client.close().await;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // New client publishes -- should not hang or error even though
    // the subscriber is gone.
    let client2 = Client::connect(ConnectOptions::new(addr)).await.unwrap();
    client2
        .publish("ephemeral", Bytes::from("data"))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    client2.close().await;
    shutdown.cancel();
}
