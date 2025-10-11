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

use casbin::{EventData, Watcher};
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

// ========== Error Types ==========

#[derive(Error, Debug)]
pub enum WatcherError {
    #[error("Redis connection error: {0}")]
    RedisConnection(#[from] redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Callback not set")]
    CallbackNotSet,

    #[error("Watcher already closed")]
    AlreadyClosed,

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Runtime error: {0}")]
    Runtime(String),
}

pub type Result<T> = std::result::Result<T, WatcherError>;

// Type aliases to reduce complexity
type UpdateCallback = Box<dyn FnMut(String) + Send + Sync>;
type CallbackArc = Arc<Mutex<Option<UpdateCallback>>>;

// ========== Message Types ==========

/// Message types for communication between watcher instances
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum UpdateType {
    Update,
    UpdateForAddPolicy,
    UpdateForRemovePolicy,
    UpdateForRemoveFilteredPolicy,
    UpdateForSavePolicy,
    UpdateForAddPolicies,
    UpdateForRemovePolicies,
    UpdateForUpdatePolicy,
    UpdateForUpdatePolicies,
}

impl std::fmt::Display for UpdateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateType::Update => write!(f, "Update"),
            UpdateType::UpdateForAddPolicy => write!(f, "UpdateForAddPolicy"),
            UpdateType::UpdateForRemovePolicy => write!(f, "UpdateForRemovePolicy"),
            UpdateType::UpdateForRemoveFilteredPolicy => write!(f, "UpdateForRemoveFilteredPolicy"),
            UpdateType::UpdateForSavePolicy => write!(f, "UpdateForSavePolicy"),
            UpdateType::UpdateForAddPolicies => write!(f, "UpdateForAddPolicies"),
            UpdateType::UpdateForRemovePolicies => write!(f, "UpdateForRemovePolicies"),
            UpdateType::UpdateForUpdatePolicy => write!(f, "UpdateForUpdatePolicy"),
            UpdateType::UpdateForUpdatePolicies => write!(f, "UpdateForUpdatePolicies"),
        }
    }
}

/// Message structure for Redis pub/sub communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Message {
    pub method: UpdateType,
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sec: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ptype: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub old_rule: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub old_rules: Vec<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_rule: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_rules: Vec<Vec<String>>,
    #[serde(default)]
    pub field_index: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_values: Vec<String>,
}

impl Message {
    pub fn new(method: UpdateType, id: String) -> Self {
        Self {
            method,
            id,
            sec: String::new(),
            ptype: String::new(),
            old_rule: Vec::new(),
            old_rules: Vec::new(),
            new_rule: Vec::new(),
            new_rules: Vec::new(),
            field_index: 0,
            field_values: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

// ========== Helper Functions ==========

/// Convert EventData to Message for publishing
fn event_data_to_message(event_data: &EventData, local_id: &str) -> Message {
    match event_data {
        EventData::AddPolicy(sec, ptype, rule) => {
            let mut message = Message::new(UpdateType::UpdateForAddPolicy, local_id.to_string());
            message.sec = sec.clone();
            message.ptype = ptype.clone();
            message.new_rule = rule.clone();
            message
        }
        EventData::AddPolicies(sec, ptype, rules) => {
            let mut message = Message::new(UpdateType::UpdateForAddPolicies, local_id.to_string());
            message.sec = sec.clone();
            message.ptype = ptype.clone();
            message.new_rules = rules.clone();
            message
        }
        EventData::RemovePolicy(sec, ptype, rule) => {
            let mut message = Message::new(UpdateType::UpdateForRemovePolicy, local_id.to_string());
            message.sec = sec.clone();
            message.ptype = ptype.clone();
            message.old_rule = rule.clone();
            message
        }
        EventData::RemovePolicies(sec, ptype, rules) => {
            let mut message =
                Message::new(UpdateType::UpdateForRemovePolicies, local_id.to_string());
            message.sec = sec.clone();
            message.ptype = ptype.clone();
            message.old_rules = rules.clone();
            message
        }
        EventData::RemoveFilteredPolicy(sec, ptype, field_values) => {
            let mut message = Message::new(
                UpdateType::UpdateForRemoveFilteredPolicy,
                local_id.to_string(),
            );
            message.sec = sec.clone();
            message.ptype = ptype.clone();
            if !field_values.is_empty() {
                message.field_values = field_values[0].clone();
            }
            message
        }
        EventData::SavePolicy(_) => {
            Message::new(UpdateType::UpdateForSavePolicy, local_id.to_string())
        }
        EventData::ClearPolicy => Message::new(UpdateType::Update, local_id.to_string()),
        EventData::ClearCache => Message::new(UpdateType::Update, local_id.to_string()),
    }
}

// ========== Redis Client Wrapper ==========

/// Wrapper to support both standalone and cluster Redis
enum RedisClientWrapper {
    Standalone(Client),
    // For Cluster mode, we also keep a standalone client for pubsub
    // because Redis Cluster doesn't have native async pubsub support
    ClusterWithPubSub {
        cluster: Box<redis::cluster::ClusterClient>,
        pubsub_client: Client,
    },
}

impl RedisClientWrapper {
    async fn get_async_pubsub(&self) -> redis::RedisResult<redis::aio::PubSub> {
        match self {
            RedisClientWrapper::Standalone(client) => client.get_async_pubsub().await,
            RedisClientWrapper::ClusterWithPubSub { pubsub_client, .. } => {
                // Use the dedicated pubsub client for cluster mode
                pubsub_client.get_async_pubsub().await
            }
        }
    }

    async fn publish_message(&self, channel: &str, payload: String) -> redis::RedisResult<()> {
        match self {
            RedisClientWrapper::Standalone(client) => {
                let mut conn = client.get_multiplexed_async_connection().await?;
                let _: i32 = conn.publish(channel, payload).await?;
                Ok(())
            }
            RedisClientWrapper::ClusterWithPubSub { cluster, .. } => {
                // Use cluster connection for publishing
                let mut conn = cluster.get_async_connection().await?;
                let _: i32 = conn.publish(channel, payload).await?;
                Ok(())
            }
        }
    }
}

// ========== Redis Watcher Implementation ==========

pub struct RedisWatcher {
    client: Arc<RedisClientWrapper>,
    options: crate::WatcherOptions,
    callback: CallbackArc,
    publish_tx: mpsc::UnboundedSender<Message>,
    publish_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    subscription_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    is_closed: Arc<AtomicBool>,
}

impl RedisWatcher {
    /// Create a new Redis watcher for standalone Redis
    pub fn new(redis_url: &str, options: crate::WatcherOptions) -> Result<Self> {
        let client = Arc::new(RedisClientWrapper::Standalone(Client::open(redis_url)?));

        // Create publish channel
        let (publish_tx, publish_rx) = mpsc::unbounded_channel::<Message>();

        let is_closed = Arc::new(AtomicBool::new(false));

        // Spawn publish task
        let publish_task = {
            let client = client.clone();
            let channel = options.channel.clone();
            let is_closed = is_closed.clone();

            tokio::spawn(async move {
                Self::publish_worker(publish_rx, client, channel, is_closed).await
            })
        };

        Ok(Self {
            client,
            options,
            callback: Arc::new(Mutex::new(None)),
            publish_tx,
            publish_task: Arc::new(Mutex::new(Some(publish_task))),
            subscription_task: Arc::new(Mutex::new(None)),
            is_closed,
        })
    }

    /// Create a new Redis watcher for Redis Cluster
    ///
    /// # Arguments
    /// * `cluster_urls` - Comma-separated list of Redis cluster nodes (e.g., "127.0.0.1:7000,127.0.0.1:7001,127.0.0.1:7002")
    /// * `options` - Watcher configuration options
    ///
    /// # Example
    /// ```no_run
    /// use redis_watcher::{RedisWatcher, WatcherOptions};
    ///
    /// let options = WatcherOptions::default();
    /// let watcher = RedisWatcher::new_cluster(
    ///     "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002",
    ///     options
    /// ).unwrap();
    /// ```
    pub fn new_cluster(cluster_urls: &str, options: crate::WatcherOptions) -> Result<Self> {
        // Parse cluster URLs
        let urls: Vec<&str> = cluster_urls.split(',').map(|s| s.trim()).collect();
        if urls.is_empty() {
            return Err(WatcherError::Configuration(
                "No cluster URLs provided".to_string(),
            ));
        }

        // Create cluster client
        let cluster_client = redis::cluster::ClusterClient::builder(urls.clone())
            .build()
            .map_err(|e| {
                WatcherError::Configuration(format!("Failed to build cluster client: {}", e))
            })?;

        // Create a standalone client for pubsub (connect to first node)
        // Redis Cluster pubsub requires connecting to a specific node
        let pubsub_client = Client::open(urls[0]).map_err(|e| {
            WatcherError::Configuration(format!("Failed to create pubsub client: {}", e))
        })?;

        let client = Arc::new(RedisClientWrapper::ClusterWithPubSub {
            cluster: Box::new(cluster_client),
            pubsub_client,
        });

        // Create publish channel
        let (publish_tx, publish_rx) = mpsc::unbounded_channel::<Message>();

        let is_closed = Arc::new(AtomicBool::new(false));

        // Spawn publish task
        let publish_task = {
            let client = client.clone();
            let channel = options.channel.clone();
            let is_closed = is_closed.clone();

            tokio::spawn(async move {
                Self::publish_worker(publish_rx, client, channel, is_closed).await
            })
        };

        Ok(Self {
            client,
            options,
            callback: Arc::new(Mutex::new(None)),
            publish_tx,
            publish_task: Arc::new(Mutex::new(Some(publish_task))),
            subscription_task: Arc::new(Mutex::new(None)),
            is_closed,
        })
    }

    /// Background worker for publishing messages
    async fn publish_worker(
        mut rx: mpsc::UnboundedReceiver<Message>,
        client: Arc<RedisClientWrapper>,
        channel: String,
        is_closed: Arc<AtomicBool>,
    ) {
        while let Some(message) = rx.recv().await {
            if is_closed.load(Ordering::Relaxed) {
                break;
            }

            if let Ok(payload) = message.to_json() {
                if let Err(e) = client.publish_message(&channel, payload).await {
                    log::error!("Failed to publish message: {}", e);
                }
            }
        }
    }

    /// Publish message to Redis channel
    fn publish_message(&self, message: &Message) -> Result<()> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(WatcherError::AlreadyClosed);
        }

        self.publish_tx
            .send(message.clone())
            .map_err(|_| WatcherError::Runtime("Publish channel closed".to_string()))?;

        Ok(())
    }

    /// Start subscription to Redis channel
    fn start_subscription(&self) -> Result<()> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(WatcherError::AlreadyClosed);
        }

        let callback = self.callback.clone();
        let channel = self.options.channel.clone();
        let local_id = self.options.local_id.clone();
        let ignore_self = self.options.ignore_self;
        let is_closed = self.is_closed.clone();
        let client = self.client.clone();

        let handle = tokio::spawn(async move {
            Self::subscription_worker(client, channel, local_id, ignore_self, is_closed, callback)
                .await
        });

        *self.subscription_task.lock().unwrap() = Some(handle);
        Ok(())
    }

    /// Background worker for subscription
    async fn subscription_worker(
        client: Arc<RedisClientWrapper>,
        channel: String,
        local_id: String,
        ignore_self: bool,
        is_closed: Arc<AtomicBool>,
        callback: CallbackArc,
    ) {
        let result = async {
            let mut pubsub = match client.get_async_pubsub().await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Failed to get async pubsub: {}", e);
                    return Err(e);
                }
            };

            if let Err(e) = pubsub.subscribe(&channel).await {
                log::error!("Failed to subscribe to channel {}: {}", channel, e);
                return Err(e);
            }

            log::debug!("Successfully subscribed to channel: {}", channel);

            let mut stream = pubsub.on_message();

            loop {
                // Check if closed before waiting for next message
                if is_closed.load(Ordering::Relaxed) {
                    break;
                }

                // Use tokio::select! to check for shutdown while waiting
                tokio::select! {
                    msg_opt = stream.next() => {
                        match msg_opt {
                            Some(msg) => {
                                let payload: String = msg.get_payload().unwrap_or_default();

                                // Parse message and check if we should ignore it
                                if ignore_self {
                                    if let Ok(parsed_msg) = Message::from_json(&payload) {
                                        if parsed_msg.id == local_id {
                                            continue;
                                        }
                                    }
                                }

                                // Call callback
                                if let Ok(mut cb_guard) = callback.lock() {
                                    if let Some(ref mut cb) = *cb_guard {
                                        cb(payload);
                                    }
                                }
                            }
                            None => {
                                // Stream ended
                                log::debug!("Pubsub stream ended");
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        // Periodic check for shutdown
                        if is_closed.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }

            Ok::<(), redis::RedisError>(())
        };

        if let Err(e) = result.await {
            log::error!("Subscription error: {}", e);
        }
    }
}

impl Watcher for RedisWatcher {
    fn set_update_callback(&mut self, cb: Box<dyn FnMut(String) + Send + Sync>) {
        *self.callback.lock().unwrap() = Some(cb);
        // Start subscription when callback is set
        let _ = self.start_subscription();
    }

    fn update(&mut self, d: EventData) {
        let message = event_data_to_message(&d, &self.options.local_id);
        let _ = self.publish_message(&message);
    }
}

impl Drop for RedisWatcher {
    fn drop(&mut self) {
        // Signal closure first
        self.is_closed.store(true, Ordering::Relaxed);

        // Abort subscription task
        if let Ok(mut handle_guard) = self.subscription_task.lock() {
            if let Some(handle) = handle_guard.take() {
                handle.abort();
            }
        }

        // Abort publish task
        if let Ok(mut handle_guard) = self.publish_task.lock() {
            if let Some(handle) = handle_guard.take() {
                handle.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let message = Message::new(UpdateType::Update, "test-id".to_string());
        let json = message.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();
        assert_eq!(message.method, parsed.method);
        assert_eq!(message.id, parsed.id);
    }

    #[test]
    fn test_event_data_conversion() {
        let event = EventData::AddPolicy(
            "p".to_string(),
            "p".to_string(),
            vec!["alice".to_string(), "data1".to_string(), "read".to_string()],
        );

        let message = event_data_to_message(&event, "test-id");
        assert_eq!(message.method, UpdateType::UpdateForAddPolicy);
        assert_eq!(message.sec, "p");
        assert_eq!(message.ptype, "p");
        assert_eq!(message.new_rule, vec!["alice", "data1", "read"]);
    }

    // Note: Integration tests that require Redis are in watcher_test.rs
}
