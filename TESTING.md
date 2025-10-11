# Redis Watcher Testing Guide

## Understanding Watcher's Responsibility

The Redis Watcher is a **notification mechanism**, not a data synchronization tool. It's important to understand what the watcher does and doesn't do:

### ✅ What Watcher DOES:
- Publish notifications when policies change
- Subscribe to a Redis channel for notifications
- Invoke callbacks when notifications are received
- Support both standalone Redis and Redis Cluster

### ❌ What Watcher DOES NOT DO:
- Synchronize actual policy data between instances
- Store or retrieve policies from a database
- Maintain a shared state between enforcers

## Distributed Policy Synchronization Architecture

In a production distributed system, policy synchronization requires **three components**:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Distributed Architecture                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐                           ┌──────────────┐    │
│  │  Enforcer 1  │                           │  Enforcer 2  │    │
│  └──────┬───────┘                           └──────┬───────┘    │
│         │                                           │            │
│         │ 1. Save policy                            │            │
│         ▼                                           │            │
│  ┌─────────────────────────────────────────────────┴───────┐   │
│  │           Shared Database (MySQL/PostgreSQL)            │   │
│  │              (DatabaseAdapter)                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│         │                                           ▲            │
│         │ 2. Publish notification                   │            │
│         ▼                                           │            │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              Redis PubSub (Watcher)                      │  │
│  └──────────────────────────────────────────────────────────┘  │
│         │                                           │            │
│         │ 3. Receive notification    4. Load policy│            │
│         └───────────────────────────────────────────┘            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Step-by-Step Flow:

1. **Enforcer 1** modifies a policy and saves it to the **shared database**
2. **Enforcer 1**'s watcher publishes a notification via **Redis PubSub**
3. **Enforcer 2**'s watcher receives the notification
4. **Enforcer 2** reloads policies from the **shared database**

## Test Categories

### 1. Notification Tests (Current)

These tests verify that the Redis PubSub mechanism works correctly:

```rust
#[tokio::test]
async fn test_watcher_notification_on_add_policy() {
    // ✅ Verifies:
    // - E1 can publish notifications
    // - E2 can receive notifications
    // - Message content is correct
    
    // ❌ Does NOT verify:
    // - Policy data synchronization
    // - Shared database access
}
```

**What we test:**
- Notification publishing works
- Notification receiving works
- Message format is correct
- ignore_self behavior works

**What we DON'T test:**
- Actual policy synchronization (requires shared database)

### 2. Integration Tests (Not Yet Implemented)

For complete end-to-end testing, you would need:

```rust
#[tokio::test]
async fn test_complete_distributed_sync_with_database() {
    // Setup shared database adapter (e.g., PostgreSQL)
    let shared_db = DatabaseAdapter::new("postgresql://...").await;
    
    // Create enforcers with shared adapter
    let mut e1 = Enforcer::new(model, shared_db.clone()).await;
    let mut e2 = Enforcer::new(model, shared_db.clone()).await;
    
    // Setup watchers
    e1.set_watcher(RedisWatcher::new(...));
    e2.set_watcher(RedisWatcher::new(...));
    
    // E1 adds policy → saves to shared DB
    e1.add_policy(vec!["alice", "data1", "read"]).await;
    
    // E2 receives notification and reloads from shared DB
    // Now both enforcers have the same policies
    assert_eq!(e1.get_policy(), e2.get_policy());
}
```

## Redis Cluster Testing

### Important: PubSub Limitation

Redis Cluster PubSub messages **DO NOT** propagate between nodes. All watcher instances **MUST** connect to the **SAME** node.

```rust
// ✅ CORRECT: All instances use the same node
let w1 = RedisWatcher::new_cluster("redis://127.0.0.1:7000", options);
let w2 = RedisWatcher::new_cluster("redis://127.0.0.1:7000", options);

// ❌ WRONG: Different nodes - messages won't propagate
let w1 = RedisWatcher::new_cluster("redis://127.0.0.1:7000", options);
let w2 = RedisWatcher::new_cluster("redis://127.0.0.1:7001", options);
```

### Cluster Test Setup

```bash
# Set environment variables
export REDIS_CLUSTER_AVAILABLE=true
export REDIS_CLUSTER_PUBSUB_NODE=redis://127.0.0.1:7000

# Run cluster tests
cargo test --lib test_redis_cluster_pubsub_notification -- --ignored --nocapture
```

## Running Tests

### Run all tests (requires Redis)
```bash
cargo test --lib
```

### Run specific test
```bash
cargo test --lib test_watcher_notification_on_add_policy -- --nocapture
```

### Run cluster tests (requires Redis Cluster)
```bash
export REDIS_CLUSTER_AVAILABLE=true
export REDIS_CLUSTER_PUBSUB_NODE=redis://127.0.0.1:7000
cargo test --lib test_redis_cluster -- --ignored --nocapture
```

### Skip tests if Redis is not available
Tests automatically skip if Redis is not available using the `is_redis_available()` helper.

## Common Issues

### Issue: "Cluster enforcers should have synced policies" assertion fails

**Cause:** This is expected behavior when using `FileAdapter`. The watcher only sends notifications, it doesn't sync policy data.

**Solution:** 
- Use a shared database adapter (DatabaseAdapter with PostgreSQL/MySQL)
- Or, only verify notification delivery (not policy equality)

### Issue: Cluster PubSub messages not received

**Cause:** Watcher instances are connecting to different Redis Cluster nodes.

**Solution:** Ensure all instances use the same `REDIS_CLUSTER_PUBSUB_NODE` URL.

### Issue: Race conditions in tests

**Cause:** Tests don't wait for subscriptions to be ready before publishing.

**Solution:** Always call `watcher.wait_for_ready().await` before performing operations.

## Best Practices

1. **Always wait for ready:** Call `wait_for_ready()` after creating watchers
2. **Use unique channels:** Use UUIDs in test channels to avoid interference
3. **Test notifications, not data sync:** Focus on verifying the messaging layer
4. **Use shared adapters for integration tests:** Only test complete sync with shared database
5. **Clean up resources:** Ensure watchers are properly closed after tests

## Example: Production Setup

```rust
use casbin::prelude::*;
use redis_watcher::{RedisWatcher, WatcherOptions};

#[tokio::main]
async fn main() -> Result<()> {
    // Setup shared database adapter
    let db_adapter = DatabaseAdapter::new("postgresql://user:pass@host/db").await?;
    
    // Create enforcer with shared adapter
    let mut enforcer = Enforcer::new("model.conf", db_adapter).await?;
    
    // Setup Redis watcher for notifications
    let options = WatcherOptions::default()
        .with_channel("casbin_policy_updates")
        .with_local_id(format!("enforcer_{}", std::process::id()));
    
    let mut watcher = RedisWatcher::new("redis://127.0.0.1:6379", options)?;
    
    // Wait for subscription to be ready
    watcher.wait_for_ready().await;
    
    // Set callback to reload policies when notified
    watcher.set_update_callback(Box::new(move |msg| {
        println!("Policy update notification received: {}", msg);
        // In real application, trigger enforcer.load_policy() here
    }));
    
    enforcer.set_watcher(Box::new(watcher));
    
    // Now any policy changes will:
    // 1. Save to shared database
    // 2. Notify all other instances via Redis
    // 3. Other instances reload from shared database
    enforcer.add_policy(vec!["alice", "data1", "read"]).await?;
    
    Ok(())
}
```

## Summary

- **Watcher = Notification System** (not data sync)
- **Tests verify notifications work** (not policy equality)
- **Production needs shared database** (for actual sync)
- **Redis Cluster requires same node** (for all instances)
- **Always wait for ready** (before operations)
