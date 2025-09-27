# Redis Watcher for Casbin-RS

<!-- Badges will be enabled once the crate is published to crates.io -->
<!-- [![Crates.io](https://img.shields.io/crates/v/casbin-redis-watcher.svg)](https://crates.io/crates/casbin-redis-watcher) -->
<!-- [![Docs.rs](https://docs.rs/casbin-redis-watcher/badge.svg)](https://docs.rs/casbin-redis-watcher) -->
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](#)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust Version](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](#)

Redis Watcher is a [Redis](http://redis.io) watcher for [Casbin-RS](https://github.com/casbin/casbin-rs). It's designed to be compatible with the Go version of casbin-redis-watcher.

## Installation

Since this crate is not yet published to crates.io, you can add it to your `Cargo.toml` as a git dependency:

```toml
[dependencies]
casbin-redis-watcher = { git = "https://github.com/casbin/casbin-rs", path = "redis-watcher-rs" }
casbin = "2.2"
tokio = { version = "1.0", features = ["full"] }
```

Or if you're working locally:

```toml
[dependencies]
casbin-redis-watcher = { path = "../path/to/redis-watcher-rs" }
casbin = "2.2"
tokio = { version = "1.0", features = ["full"] }
```

Once published to crates.io, you'll be able to use:

```toml
[dependencies]
casbin-redis-watcher = "0.1"
casbin = "2.2"
tokio = { version = "1.0", features = ["full"] }
```

## Simple Example

```rust
use casbin::prelude::*;
use casbin_redis_watcher::{RedisWatcher, WatcherOptions};
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
    // watcher.set_update_callback(casbin_redis_watcher::default_update_callback(enforcer.clone())).await?;

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

```rust
use casbin::prelude::*;
use casbin_redis_watcher::{RedisWatcher, WatcherOptions};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the watcher with Redis cluster
    let options = WatcherOptions::new()
        .with_channel("/casbin".to_string())
        .with_ignore_self(true);

    let mut watcher = RedisWatcher::new_with_cluster(
        "localhost:6379,localhost:6380,localhost:6381", 
        options
    ).await?;

    let enforcer = Enforcer::new(
        "examples/rbac_model.conf", 
        "examples/rbac_policy.csv"
    ).await?;
    
    let enforcer = Arc::new(Mutex::new(enforcer));

    // Set the watcher for the enforcer
    enforcer.lock().await.set_watcher(Box::new(watcher.clone())).await;

    // Use default callback that automatically reloads policies
    watcher.set_update_callback(
        casbin_redis_watcher::default_update_callback(enforcer.clone())
    ).await?;

    // Start listening
    watcher.start_subscription().await?;

    // Your application logic here...
    tokio::signal::ctrl_c().await?;
    watcher.close().await?;
    
    Ok(())
}
```

## API

### WatcherOptions

Configure the Redis watcher:

```rust
let options = WatcherOptions::new()
    .with_channel("/casbin".to_string())      // Redis pub/sub channel
    .with_ignore_self(true)                   // Ignore messages from this instance
    .with_local_id("my-instance".to_string()); // Custom instance ID
```

### RedisWatcher

Create watchers for standalone Redis or Redis cluster:

```rust
// Standalone Redis
let watcher = RedisWatcher::new("redis://localhost:6379", options).await?;

// Redis Cluster  
let watcher = RedisWatcher::new_with_cluster(
    "localhost:6379,localhost:6380,localhost:6381", 
    options
).await?;
```

### Update Callbacks

Set custom callback or use the default one:

```rust
// Custom callback
watcher.set_update_callback(|msg| {
    println!("Policy changed: {}", msg);
}).await?;

// Default callback (automatically reloads enforcer)
watcher.set_update_callback(
    casbin_redis_watcher::default_update_callback(enforcer_arc)
).await?;
```

## Features

- ✅ Compatible with casbin-redis-watcher (Go version)
- ✅ Support for standalone Redis and Redis Cluster
- ✅ Async/await support with Tokio
- ✅ Automatic policy reloading with default callback
- ✅ Custom update callbacks
- ✅ Message filtering (ignore self-generated messages)
- ✅ Comprehensive error handling
- ✅ Thread-safe design

## Requirements

- Rust 1.75+
- Redis 5.0+ (for standalone)
- Redis Cluster 5.0+ (for cluster mode)
- Tokio runtime

## Getting Help

- [Casbin-RS Documentation](https://docs.rs/casbin/)
- [Redis Documentation](https://redis.io/documentation)
- [GitHub Issues](https://github.com/casbin/casbin-rs/issues)

## License

This project is under Apache 2.0 License. See the [LICENSE](LICENSE) file for the full license text.
