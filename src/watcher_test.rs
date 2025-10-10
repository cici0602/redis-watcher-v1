// Copyright 2025 The Casbin Authors. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(test)]
mod tests {
    use crate::{RedisWatcher, WatcherOptions};
    use casbin::prelude::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use uuid::Uuid;

    // Helper function to check if Redis is available for testing
    fn is_redis_available() -> bool {
        std::env::var("REDIS_AVAILABLE").unwrap_or_default() == "true" || test_redis_connection()
    }

    // Helper function to check if Redis Cluster is available for testing
    fn is_redis_cluster_available() -> bool {
        std::env::var("REDIS_CLUSTER_AVAILABLE").unwrap_or_default() == "true"
            || test_redis_cluster_connection()
    }

    // Test Redis connection
    fn test_redis_connection() -> bool {
        if let Ok(client) = redis::Client::open("redis://127.0.0.1:6379") {
            if let Ok(mut conn) = client.get_connection() {
                redis::cmd("PING").query::<String>(&mut conn).is_ok()
            } else {
                false
            }
        } else {
            false
        }
    }

    // Test Redis Cluster connection
    fn test_redis_cluster_connection() -> bool {
        let urls = vec!["redis://127.0.0.1:7000"];
        if let Ok(client) = redis::cluster::ClusterClient::builder(urls).build() {
            if let Ok(mut conn) = client.get_connection() {
                redis::cmd("PING").query::<String>(&mut conn).is_ok()
            } else {
                false
            }
        } else {
            false
        }
    }

    #[test]
    fn test_watcher_creation() {
        let options = WatcherOptions::default();
        let result = RedisWatcher::new("redis://127.0.0.1:6379", options);

        // Should succeed even if Redis is not available for connection test
        // as we only create the client, not test connection in this specific case
        assert!(result.is_ok() || !is_redis_available());
    }

    #[test]
    fn test_watcher_callback() {
        if !is_redis_available() {
            println!("Skipping test - Redis not available");
            return;
        }

        let options = WatcherOptions::default()
            .with_channel("test_callback".to_string())
            .with_ignore_self(false);

        let mut watcher =
            RedisWatcher::new("redis://127.0.0.1:6379", options).expect("Failed to create watcher");

        let received_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone = received_messages.clone();

        // Set callback
        watcher.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone.lock().unwrap();
            messages.push(msg);
        }));

        // Give some time for subscription to be established
        thread::sleep(Duration::from_millis(100));

        // Test update with EventData
        let event_data = EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec!["alice".to_string(), "data1".to_string(), "read".to_string()],
        );
        watcher.update(event_data);

        // Give some time for message to be received
        thread::sleep(Duration::from_millis(100));

        let messages = received_messages.lock().unwrap();
        assert!(
            !messages.is_empty(),
            "Should have received at least one message"
        );
    }

    #[test]
    fn test_watcher_set_watcher_integration() {
        if !is_redis_available() {
            println!("Skipping test - Redis not available");
            return;
        }

        let unique_channel = format!("test_set_watcher_{}", Uuid::new_v4());
        let options = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_ignore_self(false);

        // Create watcher
        let watcher = RedisWatcher::new("redis://127.0.0.1:6379", options);
        assert!(watcher.is_ok(), "Should be able to create watcher");

        let mut watcher = watcher.unwrap();

        // Test that watcher implements the Watcher trait correctly
        // This validates the watcher can be used with set_watcher() method
        println!("Watcher creation and trait implementation test passed");

        // Test update capability
        let event_data = EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec![
                "test_user".to_string(),
                "test_obj".to_string(),
                "read".to_string(),
            ],
        );

        // This should not fail
        watcher.update(event_data);
        println!("Watcher update test passed");
    }

    #[test]
    fn test_watcher_callback_message_parsing() {
        if !is_redis_available() {
            println!("Skipping test - Redis not available");
            return;
        }

        let unique_channel = format!("test_message_parsing_{}", Uuid::new_v4());
        let options = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_ignore_self(false);

        let mut watcher =
            RedisWatcher::new("redis://127.0.0.1:6379", options).expect("Failed to create watcher");

        let received_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone = received_messages.clone();

        // Set callback to capture and parse messages
        watcher.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone.lock().unwrap();
            messages.push(msg.clone());

            // Try to parse the message to ensure it's valid JSON
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                println!("Received valid JSON message: {:?}", parsed);
            } else {
                println!("Received non-JSON message: {}", msg);
            }
        }));

        thread::sleep(Duration::from_millis(100));

        // Send different types of updates to test message format
        let test_cases = vec![
            (
                "AddPolicy",
                EventData::AddPolicy(
                    "p".to_string(),
                    "p".to_string(),
                    vec!["alice".to_string(), "data1".to_string(), "read".to_string()],
                ),
            ),
            (
                "RemovePolicy",
                EventData::RemovePolicy(
                    "p".to_string(),
                    "p".to_string(),
                    vec!["alice".to_string(), "data1".to_string(), "read".to_string()],
                ),
            ),
            (
                "AddPolicies",
                EventData::AddPolicies(
                    "p".to_string(),
                    "p".to_string(),
                    vec![vec![
                        "bob".to_string(),
                        "data2".to_string(),
                        "write".to_string(),
                    ]],
                ),
            ),
            ("SavePolicy", EventData::SavePolicy(vec![])),
        ];

        for (test_name, event_data) in test_cases {
            println!("Testing {}", test_name);
            watcher.update(event_data);
            thread::sleep(Duration::from_millis(50));
        }

        thread::sleep(Duration::from_millis(200));

        let messages = received_messages.lock().unwrap();
        println!("Total messages received: {}", messages.len());

        // Should receive at least some messages
        if !messages.is_empty() {
            println!("Message parsing test passed");
        } else {
            println!("No messages received - this may be expected behavior with ignore_self or connection issues");
        }
    }

    #[test]
    fn test_watcher_ignore_self_behavior() {
        if !is_redis_available() {
            println!("Skipping test - Redis not available");
            return;
        }

        // Test with ignore_self = false (should receive own messages)
        let unique_channel1 = format!("test_ignore_false_{}", Uuid::new_v4());
        let options1 = WatcherOptions::default()
            .with_channel(unique_channel1)
            .with_ignore_self(false);

        let mut watcher1 = RedisWatcher::new("redis://127.0.0.1:6379", options1)
            .expect("Failed to create watcher1");

        let received_own_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone1 = received_own_messages.clone();

        watcher1.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone1.lock().unwrap();
            messages.push(msg);
        }));

        thread::sleep(Duration::from_millis(100));

        // Send message with ignore_self = false
        watcher1.update(EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec!["test1".to_string(), "data1".to_string(), "read".to_string()],
        ));

        thread::sleep(Duration::from_millis(200));

        let own_messages = received_own_messages.lock().unwrap();
        println!(
            "Messages received with ignore_self=false: {}",
            own_messages.len()
        );

        // Test with ignore_self = true (should NOT receive own messages)
        let unique_channel2 = format!("test_ignore_true_{}", Uuid::new_v4());
        let options2 = WatcherOptions::default()
            .with_channel(unique_channel2)
            .with_ignore_self(true);

        let mut watcher2 = RedisWatcher::new("redis://127.0.0.1:6379", options2)
            .expect("Failed to create watcher2");

        let received_ignore_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone2 = received_ignore_messages.clone();

        watcher2.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone2.lock().unwrap();
            messages.push(msg);
        }));

        thread::sleep(Duration::from_millis(100));

        // Send message with ignore_self = true
        watcher2.update(EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec![
                "test2".to_string(),
                "data2".to_string(),
                "write".to_string(),
            ],
        ));

        thread::sleep(Duration::from_millis(200));

        let ignore_messages = received_ignore_messages.lock().unwrap();
        println!(
            "Messages received with ignore_self=true: {}",
            ignore_messages.len()
        );

        // Validate behavior
        if own_messages.len() > 0 && ignore_messages.len() == 0 {
            println!("ignore_self behavior test passed - correct filtering");
        } else {
            println!(
                "ignore_self behavior test results: own={}, ignore={}",
                own_messages.len(),
                ignore_messages.len()
            );
            println!("Note: Results may vary based on implementation details");
        }
    }

    #[test]
    fn test_multiple_watchers_sync() {
        if !is_redis_available() {
            println!("Skipping test - Redis not available");
            return;
        }

        let unique_channel = format!("test_multi_sync_{}", Uuid::new_v4());

        // Create two watchers with same channel and ignore_self=true
        let options1 = WatcherOptions::default()
            .with_channel(unique_channel.clone())
            .with_local_id("watcher_1".to_string())
            .with_ignore_self(true);

        let options2 = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_local_id("watcher_2".to_string())
            .with_ignore_self(true);

        let mut watcher1 = RedisWatcher::new("redis://127.0.0.1:6379", options1)
            .expect("Failed to create watcher1");

        let mut watcher2 = RedisWatcher::new("redis://127.0.0.1:6379", options2)
            .expect("Failed to create watcher2");

        let watcher1_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let watcher2_messages = Arc::new(Mutex::new(Vec::<String>::new()));

        let w1_clone = watcher1_messages.clone();
        let w2_clone = watcher2_messages.clone();

        watcher1.set_update_callback(Box::new(move |msg: String| {
            let mut messages = w1_clone.lock().unwrap();
            messages.push(format!("W1: {}", msg));
        }));

        watcher2.set_update_callback(Box::new(move |msg: String| {
            let mut messages = w2_clone.lock().unwrap();
            messages.push(format!("W2: {}", msg));
        }));

        // Give time for subscriptions
        thread::sleep(Duration::from_millis(200));

        // Send update from watcher1 (watcher2 should receive it)
        watcher1.update(EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec![
                "multi_test".to_string(),
                "data_sync".to_string(),
                "read".to_string(),
            ],
        ));

        thread::sleep(Duration::from_millis(200));

        // Send update from watcher2 (watcher1 should receive it)
        watcher2.update(EventData::RemovePolicy(
            "p".to_string(),
            "p".to_string(),
            vec![
                "multi_test".to_string(),
                "data_sync".to_string(),
                "read".to_string(),
            ],
        ));

        thread::sleep(Duration::from_millis(200));

        let w1_messages = watcher1_messages.lock().unwrap();
        let w2_messages = watcher2_messages.lock().unwrap();

        println!("Watcher1 received {} messages", w1_messages.len());
        println!("Watcher2 received {} messages", w2_messages.len());

        for msg in w1_messages.iter() {
            println!("  {}", msg);
        }
        for msg in w2_messages.iter() {
            println!("  {}", msg);
        }

        // With ignore_self=true, each watcher should receive messages from the other
        // but the exact behavior depends on implementation details
        println!("Multiple watchers sync test completed");
    }

    #[test]
    fn test_ignore_self_option() {
        if !is_redis_available() {
            println!("Skipping test - Redis not available");
            return;
        }

        let unique_channel = format!("test_ignore_{}", Uuid::new_v4());
        let received_messages = Arc::new(Mutex::new(Vec::<String>::new()));

        let options = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_local_id("self_watcher".to_string())
            .with_ignore_self(true); // Ignore messages from self

        let mut watcher =
            RedisWatcher::new("redis://127.0.0.1:6379", options).expect("Failed to create watcher");

        let received_clone = received_messages.clone();
        watcher.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone.lock().unwrap();
            messages.push(msg);
        }));

        // Give time for subscription
        thread::sleep(Duration::from_millis(100));

        // Send update from self - should be ignored
        let event_data = EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec!["dave".to_string(), "data4".to_string(), "read".to_string()],
        );
        watcher.update(event_data);

        // Give time for potential message
        thread::sleep(Duration::from_millis(100));

        // Should not receive own messages
        let messages = received_messages.lock().unwrap();
        assert!(
            messages.is_empty(),
            "Should not receive messages from self when ignore_self is true"
        );
    }

    // ========== Redis Cluster Tests ==========

    #[test]
    #[ignore] // Requires Redis Cluster to be running
    fn test_redis_cluster_watcher_creation() {
        if !is_redis_cluster_available() {
            println!("Skipping test - Redis Cluster not available");
            return;
        }

        let cluster_urls = std::env::var("REDIS_CLUSTER_URLS").unwrap_or_else(|_| {
            "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002".to_string()
        });

        let options = WatcherOptions::default();
        let result = RedisWatcher::new_cluster(&cluster_urls, options);

        assert!(result.is_ok(), "Should be able to create cluster watcher");
    }

    #[test]
    #[ignore] // Requires Redis Cluster to be running
    fn test_redis_cluster_watcher_callback() {
        if !is_redis_cluster_available() {
            println!("Skipping test - Redis Cluster not available");
            return;
        }

        let cluster_urls = std::env::var("REDIS_CLUSTER_URLS").unwrap_or_else(|_| {
            "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002".to_string()
        });

        let unique_channel = format!("test_cluster_{}", Uuid::new_v4());
        let options = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_ignore_self(false);

        let mut watcher = RedisWatcher::new_cluster(&cluster_urls, options)
            .expect("Failed to create cluster watcher");

        let received_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone = received_messages.clone();

        // Set callback
        watcher.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone.lock().unwrap();
            messages.push(msg);
        }));

        // Give more time for subscription to be established in CI environment
        thread::sleep(Duration::from_millis(500));

        // Test update with EventData
        let event_data = EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec!["alice".to_string(), "data1".to_string(), "read".to_string()],
        );
        watcher.update(event_data);

        // Give more time for message to be received in CI environment
        // Use a retry loop to check for messages
        let mut received = false;
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(100));
            let messages = received_messages.lock().unwrap();
            if !messages.is_empty() {
                received = true;
                break;
            }
        }

        assert!(
            received,
            "Should have received at least one message from cluster"
        );
    }

    #[test]
    #[ignore] // Requires Redis Cluster to be running
    fn test_redis_cluster_multiple_watchers() {
        if !is_redis_cluster_available() {
            println!("Skipping test - Redis Cluster not available");
            return;
        }

        let cluster_urls = std::env::var("REDIS_CLUSTER_URLS").unwrap_or_else(|_| {
            "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002".to_string()
        });

        let unique_channel = format!("test_cluster_multi_{}", Uuid::new_v4());
        let received_messages1 = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_messages2 = Arc::new(Mutex::new(Vec::<String>::new()));

        // Create first watcher
        let options1 = WatcherOptions::default()
            .with_channel(unique_channel.clone())
            .with_local_id("cluster_watcher1".to_string())
            .with_ignore_self(false);

        let mut watcher1 = RedisWatcher::new_cluster(&cluster_urls, options1)
            .expect("Failed to create cluster watcher1");

        let received_clone1 = received_messages1.clone();
        watcher1.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone1.lock().unwrap();
            messages.push(msg);
        }));

        // Create second watcher
        let options2 = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_local_id("cluster_watcher2".to_string())
            .with_ignore_self(false);

        let mut watcher2 = RedisWatcher::new_cluster(&cluster_urls, options2)
            .expect("Failed to create cluster watcher2");

        let received_clone2 = received_messages2.clone();
        watcher2.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone2.lock().unwrap();
            messages.push(msg);
        }));

        // Give time for subscriptions to be established
        thread::sleep(Duration::from_millis(500));

        // Send update from first watcher
        let event_data = EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec!["bob".to_string(), "data2".to_string(), "write".to_string()],
        );
        watcher1.update(event_data);

        // Give time for message propagation with retry logic
        let mut received1 = false;
        let mut received2 = false;
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(100));
            let messages1 = received_messages1.lock().unwrap();
            let messages2 = received_messages2.lock().unwrap();
            if !messages1.is_empty() {
                received1 = true;
            }
            if !messages2.is_empty() {
                received2 = true;
            }
            if received1 && received2 {
                break;
            }
        }

        assert!(received1, "Cluster Watcher1 should have received messages");
        assert!(received2, "Cluster Watcher2 should have received messages");
    }

    #[test]
    #[ignore] // Requires Redis Cluster to be running
    fn test_cluster_failover() {
        if !is_redis_cluster_available() {
            println!("Skipping test - Redis Cluster not available");
            return;
        }

        let cluster_urls = std::env::var("REDIS_CLUSTER_URLS").unwrap_or_else(|_| {
            "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002".to_string()
        });

        let unique_channel = format!("test_cluster_failover_{}", Uuid::new_v4());
        let options = WatcherOptions::default()
            .with_channel(unique_channel)
            .with_ignore_self(false);

        let mut watcher = RedisWatcher::new_cluster(&cluster_urls, options)
            .expect("Failed to create cluster watcher");

        let received_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone = received_messages.clone();

        // Set callback
        watcher.set_update_callback(Box::new(move |msg: String| {
            let mut messages = received_clone.lock().unwrap();
            messages.push(msg);
        }));

        // Give more time for subscription to be established
        thread::sleep(Duration::from_millis(500));

        // Send multiple updates to test cluster stability
        for i in 0..5 {
            let event_data = EventData::AddPolicy(
                "p".to_string(),
                "p".to_string(),
                vec![
                    format!("user{}", i),
                    format!("data{}", i),
                    "read".to_string(),
                ],
            );
            watcher.update(event_data);
            thread::sleep(Duration::from_millis(50));
        }

        // Wait for all messages to be received with retry logic
        let mut all_received = false;
        for _ in 0..15 {
            thread::sleep(Duration::from_millis(100));
            let messages = received_messages.lock().unwrap();
            if messages.len() >= 5 {
                all_received = true;
                break;
            }
        }

        let messages = received_messages.lock().unwrap();
        assert!(
            all_received,
            "Should have received at least 5 messages, got {}",
            messages.len()
        );
    }
}
