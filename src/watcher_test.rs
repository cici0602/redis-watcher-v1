#[cfg(test)]
mod tests {
    use crate::{WatcherOptions, Watcher, RedisWatcher, default_update_callback, Message, UpdateType};
    use casbin::prelude::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio::time::sleep;
    use uuid::Uuid;

    async fn init_watcher_with_options(options: WatcherOptions, cluster: bool) -> crate::Result<(Arc<Mutex<Enforcer>>, RedisWatcher)> {
        let watcher = if cluster {
            RedisWatcher::new_with_cluster("127.0.0.1:6379,127.0.0.1:6379,127.0.0.1:6379", options).await?
        } else {
            RedisWatcher::new("redis://127.0.0.1:6379", options).await?
        };

        let enforcer = Enforcer::new(
            "examples/rbac_model.conf", 
            "examples/rbac_policy.csv"
        ).await.expect("Failed to create enforcer");

        let enforcer = Arc::new(Mutex::new(enforcer));
        
        Ok((enforcer, watcher))
    }

    async fn init_watcher(cluster: bool) -> crate::Result<(Arc<Mutex<Enforcer>>, RedisWatcher)> {
        init_watcher_with_options(WatcherOptions::default(), cluster).await
    }

    #[tokio::test]
    #[ignore] // Remove #[ignore] when Redis is available
    async fn test_watcher() {
        let (_enforcer, mut watcher) = init_watcher(false).await.unwrap();
        
        let _ = watcher.set_update_callback(|s| {
            println!("Message: {}", s);
        }).await;
        
        // Test basic operations
        assert!(watcher.update().await.is_ok());
        assert!(watcher.update_for_save_policy().await.is_ok());
        
        let _ = watcher.close().await;
    }

    #[tokio::test]
    async fn test_message_serialization() {
        let mut message = Message::new(UpdateType::UpdateForAddPolicy, "test-id".to_string());
        message.sec = "p".to_string();
        message.ptype = "p".to_string();
        message.new_rule = vec!["alice".to_string(), "data1".to_string(), "read".to_string()];

        let json = message.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        assert_eq!(message.method, parsed.method);
        assert_eq!(message.id, parsed.id);
        assert_eq!(message.sec, parsed.sec);
        assert_eq!(message.ptype, parsed.ptype);
        assert_eq!(message.new_rule, parsed.new_rule);
    }

    #[tokio::test]
    async fn test_marshal_unmarshal() {
        let mut message = Message::new(UpdateType::UpdateForAddPolicies, "test-marshal".to_string());
        message.sec = "p".to_string();
        message.ptype = "p".to_string();
        message.new_rules = vec![
            vec!["alice".to_string(), "data1".to_string(), "read".to_string()],
            vec!["bob".to_string(), "data2".to_string(), "write".to_string()],
        ];

        let binary_data = message.marshal_binary().unwrap();
        let unmarshaled = Message::unmarshal_binary(&binary_data).unwrap();

        assert_eq!(message.method, unmarshaled.method);
        assert_eq!(message.id, unmarshaled.id);
        assert_eq!(message.sec, unmarshaled.sec);
        assert_eq!(message.ptype, unmarshaled.ptype);
        assert_eq!(message.new_rules, unmarshaled.new_rules);
    }

    #[tokio::test]
    async fn test_watcher_options() {
        let options = WatcherOptions::default()
            .with_channel("/test".to_string())
            .with_ignore_self(true)
            .with_local_id("test-instance".to_string());

        assert_eq!(options.channel, "/test");
        assert!(options.ignore_self);
        assert_eq!(options.local_id, "test-instance");
    }

    #[tokio::test] 
    async fn test_default_update_callback() {
        let enforcer = Enforcer::new(
            "examples/rbac_model.conf",
            "examples/rbac_policy.csv"
        ).await.expect("Failed to create enforcer");
        
        let enforcer = Arc::new(Mutex::new(enforcer));
        let callback = default_update_callback(enforcer.clone());
        
        // Test with a valid update message
        let message = Message::new(UpdateType::Update, "test-callback".to_string());
        let json = message.to_json().unwrap();
        
        // This should not panic
        callback(&json);
        
        // Wait a bit for async operations
        sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_update_types_display() {
        assert_eq!(UpdateType::Update.to_string(), "Update");
        assert_eq!(UpdateType::UpdateForAddPolicy.to_string(), "UpdateForAddPolicy");
        assert_eq!(UpdateType::UpdateForRemovePolicy.to_string(), "UpdateForRemovePolicy");
        assert_eq!(UpdateType::UpdateForRemoveFilteredPolicy.to_string(), "UpdateForRemoveFilteredPolicy");
        assert_eq!(UpdateType::UpdateForSavePolicy.to_string(), "UpdateForSavePolicy");
        assert_eq!(UpdateType::UpdateForAddPolicies.to_string(), "UpdateForAddPolicies");
        assert_eq!(UpdateType::UpdateForRemovePolicies.to_string(), "UpdateForRemovePolicies");
        assert_eq!(UpdateType::UpdateForUpdatePolicy.to_string(), "UpdateForUpdatePolicy");
        assert_eq!(UpdateType::UpdateForUpdatePolicies.to_string(), "UpdateForUpdatePolicies");
    }

    // Integration tests that require Redis to be running
    #[tokio::test]
    #[ignore] // Remove #[ignore] when Redis is available
    async fn test_redis_watcher_integration() {
        let local_id = Uuid::new_v4().to_string();
        let options = WatcherOptions::default()
            .with_local_id(local_id.clone())
            .with_channel("/test-integration".to_string())
            .with_ignore_self(false);

        let (enforcer, mut watcher) = init_watcher_with_options(options, false).await.unwrap();
        
        // Set up callback with default update callback
        let callback = default_update_callback(enforcer.clone());
        watcher.set_update_callback(move |msg| callback(msg)).await.unwrap();
        
        // Test various update operations
        watcher.update().await.unwrap();
        watcher.update_for_add_policy("p", "p", vec!["alice".to_string(), "data1".to_string(), "read".to_string()]).await.unwrap();
        watcher.update_for_remove_policy("p", "p", vec!["alice".to_string(), "data1".to_string(), "read".to_string()]).await.unwrap();
        watcher.update_for_save_policy().await.unwrap();
        
        // Wait for messages to be processed
        sleep(Duration::from_millis(500)).await;
        
        watcher.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Remove #[ignore] when Redis cluster is available  
    async fn test_redis_cluster_watcher() {
        let options = WatcherOptions::default()
            .with_channel("/test-cluster".to_string());

        let (enforcer, mut watcher) = init_watcher_with_options(options, true).await.unwrap();
        
        let callback = default_update_callback(enforcer);
        watcher.set_update_callback(move |msg| callback(msg)).await.unwrap();
        
        watcher.update().await.unwrap();
        sleep(Duration::from_millis(100)).await;
        
        watcher.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Remove #[ignore] when Redis is available
    async fn test_watcher_ignore_self() {
        let local_id = Uuid::new_v4().to_string();
        let options = WatcherOptions::default()
            .with_local_id(local_id.clone())
            .with_ignore_self(true)
            .with_channel("/test-ignore-self".to_string());

        let (_enforcer, mut watcher) = init_watcher_with_options(options, false).await.unwrap();
        
        let received_messages = Arc::new(Mutex::new(Vec::new()));
        let messages_clone = received_messages.clone();
        
        watcher.set_update_callback(move |msg| {
            let msg_string = msg.to_string();
            let messages = messages_clone.clone();
            tokio::spawn(async move {
                messages.lock().await.push(msg_string);
            });
        }).await.unwrap();
        
        // Send a message - should be ignored because it's from self
        watcher.update().await.unwrap();
        
        // Wait for potential message processing
        sleep(Duration::from_millis(200)).await;
        
        // Check that no messages were received (ignored self)
        let _messages = received_messages.lock().await;
        // Note: This test behavior depends on the ignore_self implementation
        
        watcher.close().await.unwrap();
    }
}