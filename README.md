# Redis Watcher for Casbin-RS

[![Crates.io](https://img.shields.io/crates/v/redis-watcher-temp.svg)](https://crates.io/crates/redis-watcher-temp)
[![Docs.rs](https://docs.rs/redis-watcher-temp/badge.svg)](https://docs.rs/redis-watcher-temp)
[![CI](https://github.com/cici0602/redis-watcher-v1/actions/workflows/ci.yml/badge.svg)](https://github.com/cici0602/redis-watcher-v1/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Redis Watcher is a [Redis](http://redis.io) watcher for [Casbin-RS](https://github.com/casbin/casbin-rs). It's designed to be compatible with the Go version of casbin-redis-watcher.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
redis-watcher-temp = "0.1"
casbin = "2.2"
tokio = { version = "1.0", features = ["full"] }
```

## Simple Example

```rust
use casbin::prelude::*;
use redis_watcher_temp::{RedisWatcher, WatcherOptions};
use std::sync::Arc;
use tokio::sync::Mutex;

fn update_callback(msg: &str) {
    println!("Policy updated: {}", msg);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the watcher with standalone Redis
    let options = WatcherOptions::new()
        .with_channel("/casbin".to_string())
        .with_ignore_self(false); // Only false in test, generally should be true

    let mut watcher = RedisWatcher::new("redis://localhost:6379", options).await?;

    // Initialize the enforcer
    let enforcer = Enforcer::new(
        "examples/rbac_model.conf", 
        "examples/rbac_policy.csv"
    ).await?;
    
    let enforcer = Arc::new(Mutex::new(enforcer));

    // Set the watcher for the enforcer
    enforcer.lock().await.set_watcher(Box::new(watcher.clone())).await;

    // Set custom callback
    watcher.set_update_callback(update_callback).await?;

    // Or use the default callback that reloads the enforcer
    // watcher.set_update_callback(redis_watcher_temp::default_update_callback(enforcer.clone())).await?;

    // Start listening for policy updates
    watcher.start_subscription().await?;

    // Update the policy to test the effect
    // You should see "Policy updated: [policy change notification]" in the log
    {
        let mut e = enforcer.lock().await;
        e.add_policy(vec!["alice".to_string(), "data1".to_string(), "read".to_string()]).await?;
        e.save_policy().await?;
    }

    // Keep the program running
    tokio::signal::ctrl_c().await?;
    watcher.close().await?;
    
    Ok(())
}
```

## Redis Cluster Example

For Redis Cluster deployment:

```rust
use casbin::prelude::*;
use redis_watcher_temp::{RedisWatcher, WatcherOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the watcher with Redis Cluster
    let options = WatcherOptions::default()
        .with_channel("/casbin-policy-updates".to_string())
        .with_ignore_self(true);

    // Provide comma-separated cluster node URLs
    let cluster_urls = "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002";
    let mut watcher = RedisWatcher::new_cluster(cluster_urls, options)?;

    // Set callback to reload policies when notified
    watcher.set_update_callback(Box::new(|msg: String| {
        println!("Received policy update from cluster: {}", msg);
        // Reload your enforcer here
    }));

    // Use with your enforcer
    // let mut enforcer = Enforcer::new("model.conf", "policy.csv")?;
    // enforcer.set_watcher(Box::new(watcher));

    Ok(())
}
```

## Running Tests

### Standalone Redis Tests

```bash
# Start Redis
docker run -d -p 6379:6379 redis:latest

# Run tests
export REDIS_AVAILABLE=true
cargo test --lib -- --include-ignored
```

### Redis Cluster Tests

```bash
# Setup Redis Cluster (see CI configuration for detailed setup)
# Or use the setup script provided

# Run cluster tests
export REDIS_CLUSTER_AVAILABLE=true
export REDIS_CLUSTER_URLS=redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002
cargo test test_redis_cluster -- --ignored
```

## API

### WatcherOptions

Configure the Redis watcher:

```rust
let options = WatcherOptions::default()
    .with_channel("/casbin-policy-updates".to_string())  // Redis pub/sub channel
    .with_ignore_self(true)                              // Ignore messages from this instance
    .with_local_id("my-instance".to_string());           // Custom instance ID (auto-generated if not set)
```

### RedisWatcher

Create watchers for standalone Redis or Redis cluster:

```rust
// Standalone Redis
let watcher = RedisWatcher::new("redis://localhost:6379", options)?;

// Redis Cluster (comma-separated node URLs)
let cluster_urls = "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002";
let watcher = RedisWatcher::new_cluster(cluster_urls, options)?;
```

### Update Callbacks

Set a custom callback to handle policy updates:

```rust
watcher.set_update_callback(Box::new(|msg: String| {
    println!("Policy changed: {}", msg);
    // Parse the message and reload your enforcer
}));
```

## Features

- ✅ Compatible with casbin-redis-watcher (Go version)
- ✅ Support for standalone Redis and Redis Cluster
- ✅ Synchronous design for easy integration
- ✅ Automatic policy change notifications via Redis pub/sub
- ✅ Custom update callbacks
- ✅ Message filtering (ignore self-generated messages)
- ✅ Comprehensive error handling with thiserror
- ✅ Thread-safe design with proper cleanup

## Requirements

- Rust 1.82+
- Redis 5.0+ (for standalone mode)
- Redis Cluster 5.0+ (for cluster mode)

## CI/CD

This project includes comprehensive CI/CD pipelines:

- **Unit tests**: Run on all platforms (Linux, macOS, Windows)
- **Integration tests**: 
  - Standalone Redis tests on Linux with Redis service
  - Redis Cluster tests on Linux with 6-node cluster setup
- **Cross-platform support**: Tested on multiple Rust versions (1.82+, stable)

## Getting Help

- [Casbin-RS Documentation](https://docs.rs/casbin/)
- [Redis Documentation](https://redis.io/documentation)
- [GitHub Issues](https://github.com/casbin/casbin-rs/issues)

## License

This project is under Apache 2.0 License. See the [LICENSE](LICENSE) file for the full license text.
