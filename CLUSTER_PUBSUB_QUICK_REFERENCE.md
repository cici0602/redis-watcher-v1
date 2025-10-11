# Redis Cluster PubSub 快速参考

## 🚨 最重要的规则

**Redis Cluster PubSub 消息不会在节点之间传播！**

所有实例必须连接到**同一个**节点才能通信。

## ✅ 正确用法

### 单节点配置（推荐）

```rust
// 所有实例使用相同的单节点 URL
let pubsub_node = "redis://127.0.0.1:7000";

let watcher1 = RedisWatcher::new_cluster(pubsub_node, options1)?;
let watcher2 = RedisWatcher::new_cluster(pubsub_node, options2)?;
let watcher3 = RedisWatcher::new_cluster(pubsub_node, options3)?;
```

### 环境变量配置

```bash
# .env 或环境变量
REDIS_PUBSUB_NODE=redis://pubsub.example.com:7000
```

```rust
let pubsub_node = std::env::var("REDIS_PUBSUB_NODE")
    .unwrap_or_else(|_| "redis://127.0.0.1:7000".to_string());
    
let watcher = RedisWatcher::new_cluster(&pubsub_node, options)?;
```

### Docker Compose 示例

```yaml
version: '3.8'

services:
  app1:
    environment:
      - REDIS_PUBSUB_NODE=redis://redis-node-1:7000
      
  app2:
    environment:
      - REDIS_PUBSUB_NODE=redis://redis-node-1:7000  # 同一节点
      
  redis-node-1:
    image: redis:alpine
    ports:
      - "7000:7000"
```

### Kubernetes ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: redis-config
data:
  REDIS_PUBSUB_NODE: "redis://redis-cluster-node-1:7000"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: casbin-app
spec:
  template:
    spec:
      containers:
      - name: app
        envFrom:
        - configMapRef:
            name: redis-config
```

## ❌ 错误用法

### 错误 1: 不同的节点

```rust
// ❌ 实例1连接到7000，实例2连接到7001
// 它们无法通信！
let watcher1 = RedisWatcher::new_cluster("redis://127.0.0.1:7000", options1)?;
let watcher2 = RedisWatcher::new_cluster("redis://127.0.0.1:7001", options2)?;
```

### 错误 2: 多节点 URL（误导性）

```rust
// ⚠️  虽然使用第一个节点，但容易混淆
// 不如显式使用单节点
let urls = "redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002";
let watcher = RedisWatcher::new_cluster(urls, options)?;
```

### 错误 3: 假设会自动同步

```rust
// ❌ 错误假设：认为集群会自动处理 PubSub
// 实际：必须手动确保连接到同一节点
```

## 🏗️ 生产环境架构

### 方案 1: 专用 PubSub 节点

```
┌─────────────┐
│ PubSub Node │ ← 所有实例连接到此节点
│ (Redis 7000)│
└─────────────┘
      ↑ ↑ ↑
      │ │ │
   ┌──┘ │ └──┐
   │    │    │
┌──┴─┐ ┌┴──┐ ┌┴───┐
│App1│ │App2│ │App3│
└────┘ └───┘ └────┘
```

优点：
- 简单明确
- 容易监控
- 性能可预测

缺点：
- 单点（需要配合 Sentinel）

### 方案 2: Redis Sentinel + PubSub

```
┌──────────────────┐
│ Redis Sentinel   │ ← 提供高可用
│ (自动故障转移)   │
└────────┬─────────┘
         │
    ┌────┴────┐
    │ Master  │ ← PubSub 节点
    └────┬────┘
         │
    ┌────┴────┐
    │ Replica │ ← 故障时自动提升
    └─────────┘
```

```rust
// 使用 Sentinel 配置
let sentinel_url = "redis-sentinel://sentinel1:26379,sentinel2:26379/mymaster";
let watcher = RedisWatcher::new_cluster(&sentinel_url, options)?;
```

### 方案 3: 负载均衡器（不推荐）

```
┌──────────────┐
│ Load Balancer│ ← ⚠️ 必须是 TCP 层，session sticky
└──────┬───────┘
       │
  ┌────┴────┐
  │  Redis  │
  └─────────┘
```

⚠️ 注意：负载均衡器必须保持连接到同一后端

## 🧪 测试环境配置

### GitHub Actions

```yaml
env:
  REDIS_CLUSTER_PUBSUB_NODE: redis://127.0.0.1:7000

steps:
  - name: Run Cluster Tests
    run: cargo test test_redis_cluster_enforcer_sync -- --ignored
```

### 本地测试

```bash
# 启动 Redis Cluster
docker-compose up -d

# 设置环境变量
export REDIS_CLUSTER_PUBSUB_NODE=redis://127.0.0.1:7000

# 运行测试
cargo test --lib -- --ignored
```

## 🔍 故障排查

### 症状：回调不触发

```
✗ Failed to receive callback after 10 attempts
Callback received flag: false
```

**可能原因：**
1. 不同实例连接到不同节点
2. 网络问题
3. Redis 节点宕机

**检查步骤：**

```bash
# 1. 检查所有实例的日志，确认连接到同一节点
grep "PubSub using fixed node" app.log
# 应该看到相同的节点地址

# 2. 检查 Redis 连接
redis-cli -h 127.0.0.1 -p 7000 PING

# 3. 监控 PubSub 通道
redis-cli -h 127.0.0.1 -p 7000 SUBSCRIBE /casbin
# 然后在另一个终端发布消息测试
```

### 症状：间歇性失败

**可能原因：**
- 负载均衡器路由到不同节点
- 容器重启连接到不同节点
- DNS 轮询

**解决方案：**
- 使用固定的 IP 地址而不是主机名
- 配置负载均衡器 session affinity
- 使用 Sentinel 确保一致性

## 📚 相关文档

- [完整分析](./CLUSTER_PUBSUB_ANALYSIS.md)
- [修复总结](./CLUSTER_PUBSUB_FIX_SUMMARY.md)
- [README](./README.md)

## 🆘 获取帮助

如果仍然有问题：
1. 检查所有实例的 `REDIS_CLUSTER_PUBSUB_NODE` 配置
2. 启用 `RUST_LOG=debug` 查看详细日志
3. 使用 `redis-cli MONITOR` 观察 Redis 命令
4. 提交 Issue 并附带日志
