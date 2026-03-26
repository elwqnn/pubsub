//! Interactive chat client.
//!
//! Run with: `cargo run --example chat-client -p pubsub -- --name alice --room general`

use std::time::Duration;

use futures::StreamExt;
use tokio::io::{AsyncBufReadExt, BufReader};

use pubsub::{Client, ConnectOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let name = find_arg(&args, "--name").unwrap_or_else(|| "anon".into());
    let room = find_arg(&args, "--room").unwrap_or_else(|| "general".into());
    let addr = find_arg(&args, "--addr").unwrap_or_else(|| "127.0.0.1:4222".into());

    let client = Client::connect(ConnectOptions::from_host(&addr)).await?;

    // Join the room.
    client
        .publish(&format!("chat.join.{}", room), name.as_str())
        .await?;

    // Subscribe to room messages.
    let mut sub = client.subscribe(&format!("chat.room.{}", room)).await?;

    println!("=== Joined #{} as {} ===", room, name);
    println!("Type a message and press Enter. Commands: /rooms, /quit\n");

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        tokio::select! {
            // Incoming message from the room.
            Some(msg) = sub.next() => {
                let text = String::from_utf8_lossy(&msg.payload);
                println!("{}", text);
            }

            // User typed a line.
            result = lines.next_line() => {
                let line = match result {
                    Ok(Some(line)) => line,
                    _ => break,
                };

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                match trimmed {
                    "/quit" => {
                        client
                            .publish(&format!("chat.leave.{}", room), name.as_str())
                            .await?;
                        break;
                    }

                    "/rooms" => {
                        match client.request("chat.rooms", "", Duration::from_secs(2)).await {
                            Ok(reply) => {
                                let list = String::from_utf8_lossy(&reply.payload);
                                println!("--- Active rooms ---\n{}\n--------------------", list);
                            }
                            Err(e) => println!("Failed to list rooms: {}", e),
                        }
                    }

                    _ => {
                        let formatted = format!("{}: {}", name, trimmed);
                        client
                            .publish(&format!("chat.room.{}", room), formatted)
                            .await?;
                    }
                }
            }
        }
    }

    let _ = sub.unsubscribe().await;
    client.close().await;
    println!("Disconnected.");
    Ok(())
}

fn find_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
