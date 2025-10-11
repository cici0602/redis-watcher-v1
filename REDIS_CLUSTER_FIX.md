# Redis Cluster PubSub 修复说明

## 问题分析

### 核心问题
Redis Cluster 集成测试失败，E2 无法接收到 E1 发布的策略更新通知。

### 根本原因
**Redis Cluster 的 PubSub 机制限制**：
- Redis Cluster 中的 PubSub 消息**不会**在集群节点之间自动传播
- 订阅者必须连接到**发布者所在的同一个节点**才能接收消息
- 当 E1 和 E2 连接到不同的集群节点时，消息会丢失

### 原实现的问题
```rust
// 之前的实现
ClusterWithPubSub {
    cluster: Box<redis::cluster::ClusterClient>,  // 用于发布
    pubsub_client: Client,                         // 用于订阅（第一个节点）
}
```

**问题**：
1. 使用 `ClusterClient` 发布消息时，可能被路由到不同的节点
2. 使用固定的 `pubsub_client`（第一个节点）订阅
3. 如果发布到节点 A，但订阅在节点 B，消息永远无法到达

## 解决方案

### 实现策略
**确保发布和订阅在同一个节点上**：

```rust
// 修复后的实现
ClusterPubSub {
    pubsub_client: Client,  // 同一个客户端用于发布和订阅
}
```

### 关键改动

#### 1. 简化客户端包装器
```rust
enum RedisClientWrapper {
    Standalone(Client),
    ClusterPubSub {
        pubsub_client: Client,  // 使用单一节点处理所有 PubSub 操作
    },
}
```

#### 2. 统一发布和订阅到同一节点
```rust
impl RedisClientWrapper {
    async fn publish_message(&self, channel: &str, payload: String) -> redis::RedisResult<()> {
        match self {
            RedisClientWrapper::ClusterPubSub { pubsub_client } => {
                // 使用相同的 pubsub_client 进行发布
                let mut conn = pubsub_client.get_multiplexed_async_connection().await?;
                let _: i32 = conn.publish(channel, payload).await?;
                Ok(())
            }
            // ...
        }
    }

    async fn get_async_pubsub(&self) -> redis::RedisResult<redis::aio::PubSub> {
        match self {
            RedisClientWrapper::ClusterPubSub { pubsub_client } => {
                // 使用相同的 pubsub_client 进行订阅
                pubsub_client.get_async_pubsub().await
            }
            // ...
        }
    }
}
```

#### 3. 增强测试日志
- 添加消息内容追踪
- 改进失败时的错误信息
- 增加调试输出

## 与 Go 实现的对比

### Go 实现（casbin-go）
```go
func NewWatcherWithCluster(addrs string, option WatcherOptions) (persist.Watcher, error) {
    addrsStr := strings.Split(addrs, ",")
    w := &Watcher{
        subClient: rds.NewClusterClient(&rds.ClusterOptions{Addrs: addrsStr}),
        pubClient: rds.NewClusterClient(&rds.ClusterOptions{Addrs: addrsStr}),
    }
    // ...
    w.subscribe()
    return w, nil
}
```

**Go 版本的注意事项**：
- `go-redis` 的 `ClusterClient.Subscribe()` 实际上也会连接到单个节点
- 虽然使用了两个 ClusterClient，但订阅操作本质上是在一个节点上
- 发布时，如果使用 `PUBLISH` 命令，也会被路由到特定节点

### Rust 实现的优势
- **明确性**：清楚地表明 PubSub 使用单节点连接
- **简洁性**：移除了不必要的 ClusterClient
- **性能**：减少了不必要的集群连接开销

## 测试改进

### 增强的测试日志
```rust
// 捕获消息内容以便调试
let message_content = Arc::new(Mutex::new(String::new()));
let message_clone = message_content.clone();

w2.set_update_callback(Box::new(move |msg: String| {
    println!("[Cluster E2] Received update notification: {}", msg);
    *message_clone.lock().unwrap() = msg;
    *callback_clone.lock().unwrap() = true;
}));
```

### 更好的失败诊断
```rust
if !received {
    eprintln!("✗ Failed to receive callback after 10 attempts");
    eprintln!("Last message content: {}", msg);
    eprintln!("Callback received flag: {}", received);
}

assert!(
    received,
    "Cluster E2 should receive update notification. Check:\n\
     1. Both watchers connect to the same Redis node\n\
     2. Channel name matches: {}\n\
     3. Redis Cluster is properly configured\n\
     4. Check logs above for publish/subscribe details",
    channel_for_error
);
```

## CI 配置验证

### Redis Cluster 设置（.github/workflows/ci.yml）
```yaml
- name: Setup Redis Cluster
  if: matrix.redis-mode == 'cluster'
  run: |
    # 创建 6 节点集群（3 主 3 从）
    for port in 7000 7001 7002 7003 7004 7005; do
      cat > /tmp/redis-cluster/$port/redis.conf <<EOF
    port $port
    cluster-enabled yes
    cluster-config-file nodes-$port.conf
    cluster-node-timeout 5000
    appendonly yes
    bind 0.0.0.0
    protected-mode no
    EOF
      redis-server /tmp/redis-cluster/$port/redis.conf --daemonize yes
    done
    
    # 创建集群
    echo "yes" | redis-cli --cluster create \
      127.0.0.1:{7000,7001,7002,7003,7004,7005} \
      --cluster-replicas 1
```

### 环境变量
```yaml
env:
  REDIS_CLUSTER_AVAILABLE: ${{ matrix.redis-mode == 'cluster' }}
  REDIS_CLUSTER_URLS: redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002
```

## 使用建议

### 1. Standalone Redis（推荐用于开发）
```rust
let watcher = RedisWatcher::new("redis://127.0.0.1:6379", options)?;
```

### 2. Redis Cluster（生产环境）
```rust
let watcher = RedisWatcher::new_cluster(
    "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002",
    options
)?;
```

**注意**：
- 所有 watcher 实例将使用第一个 URL 进行 PubSub 通信
- 确保该节点稳定可用
- 如果需要高可用性，可以考虑在代码中实现故障转移

## 性能考虑

### 优点
- 减少连接数：每个 watcher 只需一个连接
- 简化逻辑：无需处理消息路由
- 降低延迟：直接节点通信

### 缺点
- 单点依赖：所有 PubSub 流量通过一个节点
- 可扩展性限制：该节点可能成为瓶颈

### 替代方案（未来考虑）
1. **Broadcast 模式**：向所有主节点发布消息
2. **Redis Streams**：使用更适合集群的消息传递机制
3. **外部消息队列**：使用 Kafka/RabbitMQ 等

## 验证步骤

### 本地测试
```bash
# 启动 Redis Cluster
docker run -d --name redis-cluster -p 7000-7005:7000-7005 \
  grokzen/redis-cluster:latest

# 运行测试
REDIS_CLUSTER_AVAILABLE=true \
REDIS_CLUSTER_URLS=redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002 \
cargo test test_redis_cluster_enforcer_sync -- --ignored --nocapture
```

### CI 测试
测试会在 GitHub Actions 中自动运行：
- Linux + Redis Cluster
- Rust 1.82 和 stable 版本

## 参考资料

- [Redis Cluster Pub/Sub](https://redis.io/docs/reference/cluster-spec/#pubsub)
- [redis-rs Documentation](https://docs.rs/redis/latest/redis/)
- [Casbin Watcher Pattern](https://casbin.org/docs/watchers)
