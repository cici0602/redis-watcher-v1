//! Redis Watcher for Casbin-RS
//!
//! This library provides Redis-based policy change notifications for Casbin-RS.
//! It's designed to be compatible with the Go version of casbin-redis-watcher.

mod options;
mod watcher;

#[cfg(test)]
mod watcher_test;

pub use options::WatcherOptions;
pub use watcher::{default_update_callback, RedisWatcher, Watcher};

/// Re-export for convenience
pub use watcher::{Message, Result, UpdateType, WatcherError};
