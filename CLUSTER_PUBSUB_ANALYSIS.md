# Redis Cluster PubSub 问题分析与修复方案

## 问题诊断

### 测试失败现象
```
Add policy result: Ok(false)
✗ Failed to receive callback after 10 attempts
Callback received flag: false
```

### 根本原因分析

**Redis Cluster 的 PubSub 特性限制：**

1. **消息不跨节点传播**：Redis Cluster 中的 PubSub 消息**不会**在集群节点之间传播
2. **订阅者必须连接到发布者所在的节点**：只有连接到同一个节点的订阅者才能收到消息
3. **数据分片不影响 PubSub**：PubSub 与数据的 hash slot 分配无关，完全基于物理连接

### Go 版本的正确实现

查看 Go 代码 `watcher.go`：

```go
// NewWatcherWithCluster creates a new Watcher to be used with a Casbin enforcer
func NewWatcherWithCluster(addrs string, option WatcherOptions) (persist.Watcher, error) {
    addrsStr := strings.Split(addrs, ",")
    option.ClusterOptions.Addrs = addrsStr
    
    w := &Watcher{
        // 关键：为 pubsub 和发布使用相同的 ClusterClient
        subClient: rds.NewClusterClient(&rds.ClusterOptions{
            Addrs:    addrsStr,
            Password: option.ClusterOptions.Password,
        }),
        pubClient: rds.NewClusterClient(&rds.ClusterOptions{
            Addrs:    addrsStr,
            Password: option.ClusterOptions.Password,
        }),
        ctx:   context.Background(),
        close: make(chan struct{}),
    }
    // ...
}
```

**Go 的 redis-go-client 行为：**
- `ClusterClient` 会自动选择一个节点建立连接
- 同一个进程内的多个 `ClusterClient` 实例会连接到**不同的节点**（负载均衡）
- 这导致 publish 和 subscribe 可能在不同节点上，消息无法传递

### Rust 当前实现的问题

```rust
pub fn new_cluster(cluster_urls: &str, options: crate::WatcherOptions) -> Result<Self> {
    let urls: Vec<&str> = cluster_urls.split(',').map(|s| s.trim()).collect();
    
    // ❌ 问题：每个 watcher 实例可能连接到不同的节点
    // Watcher1 的 pubsub_client -> 节点 7000
    // Watcher2 的 pubsub_client -> 节点 7001
    // 结果：Watcher1 发布到 7000，Watcher2 订阅 7001，收不到消息！
    
    let pubsub_client = Client::open(urls[0])?;  // 第一个节点
    
    let client = Arc::new(RedisClientWrapper::ClusterPubSub { pubsub_client });
    // ...
}
```

## 解决方案

### 方案选择

**❌ 方案1：使用 Redis Cluster 的 PUBLISH 命令**
- 问题：redis-rs 的 PubSub API 不支持 ClusterClient

**✅ 方案2：确保所有实例连接到同一个固定节点**
- 优点：简单可靠，与 Go 版本一致
- 实现：使用环境变量或配置指定固定的 PubSub 节点

**✅ 方案3：在测试中显式指定同一节点**
- 优点：测试更可控
- 实现：测试代码中使用相同的单节点 URL

### 推荐实现：方案2 + 方案3

1. **代码层面**：支持指定固定的 PubSub 节点（用于生产环境）
2. **测试层面**：显式使用同一节点（用于 CI 测试）

## 修复计划

### 1. 修改 `watcher.rs`

```rust
/// Create a new Redis watcher for Redis Cluster
///
/// **Important for Redis Cluster PubSub:**
/// Redis Cluster PubSub messages do NOT propagate between nodes.
/// All watcher instances MUST connect to the SAME node for pub/sub to work.
///
/// # Arguments
/// * `cluster_urls` - Comma-separated Redis URLs. First URL will be used for PubSub.
///   All instances must use the SAME first URL.
///
/// # Example
/// ```no_run
/// // ✅ Correct: All instances use the same first URL
/// let watcher1 = RedisWatcher::new_cluster(
///     "redis://127.0.0.1:7000,redis://127.0.0.1:7001",
///     options1
/// )?;
/// let watcher2 = RedisWatcher::new_cluster(
///     "redis://127.0.0.1:7000,redis://127.0.0.1:7001",  // Same first URL
///     options2
/// )?;
/// ```
pub fn new_cluster(cluster_urls: &str, options: crate::WatcherOptions) -> Result<Self> {
    // Parse cluster URLs
    let urls: Vec<&str> = cluster_urls.split(',').map(|s| s.trim()).collect();
    
    // Use first URL for PubSub - all instances must use the same URL
    let pubsub_url = urls[0];
    let pubsub_client = Client::open(pubsub_url)?;
    
    log::info!(
        "Redis Cluster PubSub using fixed node: {} (All instances must use the same node!)",
        pubsub_url
    );
    
    // Rest of implementation...
}
```

### 2. 修改测试代码

```rust
#[tokio::test]
#[ignore]
async fn test_redis_cluster_enforcer_sync() {
    // 使用固定的单节点 URL 确保所有实例连接到同一节点
    let pubsub_node = "redis://127.0.0.1:7000";
    
    println!("Using fixed Redis Cluster PubSub node: {}", pubsub_node);
    println!("⚠️  All watcher instances MUST connect to the same node for cluster PubSub!");
    
    let wo1 = WatcherOptions::default()
        .with_channel(unique_channel.clone())
        .with_local_id("cluster_enforcer1".to_string())
        .with_ignore_self(true);

    let wo2 = WatcherOptions::default()
        .with_channel(unique_channel)
        .with_local_id("cluster_enforcer2".to_string())
        .with_ignore_self(true);
    
    // ✅ 两个 watcher 使用相同的节点
    let mut w1 = RedisWatcher::new_cluster(pubsub_node, wo1)?;
    let mut w2 = RedisWatcher::new_cluster(pubsub_node, wo2)?;
    
    // Rest of test...
}
```

### 3. 更新 CI 配置

```yaml
- name: Run Integration Tests
  env:
    REDIS_CLUSTER_PUBSUB_NODE: redis://127.0.0.1:7000  # 固定节点
  run: |
    cargo test test_redis_cluster_enforcer_sync -- --ignored --nocapture
```

## 技术细节

### Redis Cluster PubSub 工作原理

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│  Node 7000  │         │  Node 7001  │         │  Node 7002  │
│             │         │             │         │             │
│  ❌ PubSub  │────X────│  PubSub ❌  │────X────│  PubSub ❌  │
│  messages   │  不传播  │  messages   │  不传播  │  messages   │
│  isolated   │         │  isolated   │         │  isolated   │
└─────────────┘         └─────────────┘         └─────────────┘

正确做法：所有实例连接到同一节点
┌─────────────┐
│  Node 7000  │
│             │
│  Publisher  │◄──┐
│      ↓      │   │
│  Subscriber1│   │ 所有实例
│  Subscriber2│◄──┤ 连接到
│  Subscriber3│◄──┘ 同一节点
└─────────────┘
```

### 为什么 Go 实现也有这个问题？

Go 实现虽然使用了 `ClusterClient`，但在实际运行时：
- 每个进程创建的 `ClusterClient` 会连接到不同节点
- 需要通过配置或环境变量确保连接到同一节点
- 文档中应该说明这个限制

## 总结

这是一个**业务逻辑设计问题**，而不是测试或 CI 问题：

1. **根本原因**：没有正确处理 Redis Cluster 的 PubSub 限制
2. **解决方案**：确保所有 watcher 实例连接到同一个 Redis 节点进行 PubSub
3. **最佳实践**：
   - 在文档中明确说明 Redis Cluster PubSub 的限制
   - 提供配置选项指定固定的 PubSub 节点
   - 测试代码中显式使用同一节点
   - 添加警告日志提醒用户
