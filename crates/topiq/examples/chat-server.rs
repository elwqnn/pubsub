//! Chat server: embedded broker + chat service.
//!
//! Run with: `cargo run --example chat-server -p topiq --features server`

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::signal;
use tokio::sync::Mutex;

use topiq::server::{
    AckTracker, BrokerConfig, CancellationToken, Router, SubscriptionRegistry,
    TcpTransportListener,
};
use topiq::{Client, ConnectOptions};

/// Shared state for tracking rooms and users.
struct ChatState {
    /// room -> set of usernames
    rooms: HashMap<String, HashSet<String>>,
}

impl ChatState {
    fn new() -> Self {
        Self {
            rooms: HashMap::new(),
        }
    }

    fn join(&mut self, room: &str, user: &str) {
        self.rooms
            .entry(room.to_owned())
            .or_default()
            .insert(user.to_owned());
    }

    fn leave(&mut self, room: &str, user: &str) {
        if let Some(users) = self.rooms.get_mut(room) {
            users.remove(user);
            if users.is_empty() {
                self.rooms.remove(room);
            }
        }
    }

    fn room_list(&self) -> String {
        if self.rooms.is_empty() {
            return "No active rooms".to_owned();
        }
        self.rooms
            .iter()
            .map(|(room, users)| format!("  {} ({} users)", room, users.len()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = "127.0.0.1:4222";
    let shutdown = CancellationToken::new();

    // --- Start embedded broker ---
    let registry = Arc::new(SubscriptionRegistry::new());
    let config = BrokerConfig {
        bind_addr: bind_addr.parse()?,
        ..Default::default()
    };
    let ack_tracker = Arc::new(AckTracker::new(config.ack_timeout, config.max_redeliveries));
    let router = Arc::new(Router::new(registry.clone(), ack_tracker.clone()));

    let listener =
        TcpTransportListener::bind(&config, router, registry, ack_tracker, shutdown.clone())
            .await?;
    let addr = listener.local_addr()?;

    let listener_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = listener.run().await;
        listener_shutdown.cancel();
    });

    println!("=== Chat Server listening on {} ===", addr);

    // Small delay for the listener to be ready.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // --- Connect as a service client ---
    let client = Client::connect(ConnectOptions::new(addr)).await?;
    let state = Arc::new(Mutex::new(ChatState::new()));

    // Subscribe to all chat events.
    let mut join_sub = client.subscribe("chat.join.>").await?;
    let mut leave_sub = client.subscribe("chat.leave.>").await?;
    let mut rooms_sub = client.subscribe("chat.rooms").await?;
    let mut msg_sub = client.subscribe("chat.room.>").await?;

    println!("Chat service ready. Waiting for clients...\n");

    let service_shutdown = shutdown.clone();
    loop {
        tokio::select! {
            Some(msg) = join_sub.next() => {
                let room = extract_last_token(msg.topic.as_str());
                let user = String::from_utf8_lossy(&msg.payload).to_string();
                state.lock().await.join(&room, &user);
                println!("[{}] {} joined", room, user);

                // Announce to the room.
                let announcement = format!("* {} has joined the room", user);
                let _ = client.publish(&format!("chat.room.{}", room), announcement).await;
            }

            Some(msg) = leave_sub.next() => {
                let room = extract_last_token(msg.topic.as_str());
                let user = String::from_utf8_lossy(&msg.payload).to_string();
                state.lock().await.leave(&room, &user);
                println!("[{}] {} left", room, user);

                let announcement = format!("* {} has left the room", user);
                let _ = client.publish(&format!("chat.room.{}", room), announcement).await;
            }

            Some(msg) = rooms_sub.next() => {
                if let Some(reply_to) = &msg.reply_to {
                    let list = state.lock().await.room_list();
                    let _ = client.publish(reply_to.as_str(), list).await;
                }
            }

            Some(msg) = msg_sub.next() => {
                let room = extract_last_token(msg.topic.as_str());
                let text = String::from_utf8_lossy(&msg.payload);
                println!("[{}] {}", room, text);
            }

            _ = signal::ctrl_c() => {
                println!("\nShutting down...");
                shutdown.cancel();
                break;
            }

            _ = service_shutdown.cancelled() => {
                break;
            }
        }
    }

    client.close().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("Server stopped.");
    Ok(())
}

/// Extract the last dot-separated token from a subject.
/// e.g., "chat.join.general" -> "general"
fn extract_last_token(subject: &str) -> String {
    subject
        .rsplit('.')
        .next()
        .unwrap_or(subject)
        .to_owned()
}
