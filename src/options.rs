use uuid::Uuid;

/// Configuration options for the Redis watcher
/// This mirrors the Go version's WatcherOptions structure
#[derive(Debug, Clone)]
pub struct WatcherOptions {
    /// Redis channel for pub/sub (equivalent to Go's Channel)
    pub channel: String,

    /// Whether to ignore messages from self (equivalent to Go's IgnoreSelf)
    pub ignore_self: bool,

    /// Local instance ID (equivalent to Go's LocalID)  
    pub local_id: String,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            channel: "/casbin".to_string(),
            ignore_self: false,
            local_id: Uuid::new_v4().to_string(),
        }
    }
}

impl WatcherOptions {
    /// Create new WatcherOptions with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the Redis pub/sub channel
    pub fn with_channel(mut self, channel: String) -> Self {
        self.channel = channel;
        self
    }

    /// Set whether to ignore self messages
    pub fn with_ignore_self(mut self, ignore_self: bool) -> Self {
        self.ignore_self = ignore_self;
        self
    }

    /// Set local instance ID
    pub fn with_local_id(mut self, local_id: String) -> Self {
        self.local_id = local_id;
        self
    }
}
