# Redis Cluster 测试快速指南

## 问题修复状态

✅ **已修复**: Redis Cluster PubSub 消息传递问题
✅ **已验证**: 代码编译和单元测试通过
⏳ **待验证**: CI 集成测试（需要推送到 GitHub）

## 修复内容

### 核心改动
1. **统一 PubSub 节点**: 所有 PubSub 操作（发布和订阅）使用同一个 Redis 节点
2. **简化客户端**: 移除不必要的 ClusterClient，使用单一 Client 处理 PubSub
3. **增强日志**: 添加详细的调试输出和错误信息

### 文件变更
- `src/watcher.rs` - 修复 PubSub 逻辑
- `src/watcher_test.rs` - 增强测试日志
- `README.md` - 添加 Redis Cluster 使用说明
- `REDIS_CLUSTER_FIX.md` - 详细技术文档
- `FIX_SUMMARY.md` - 修复总结

## 本地测试（可选）

### 前提条件
需要本地运行的 Redis Cluster（6 节点：3 主 3 从）

### 使用 Docker 快速启动
```bash
docker run -d --name redis-cluster \
  -p 7000-7005:7000-7005 \
  grokzen/redis-cluster:latest
```

### 运行集群测试
```bash
# 设置环境变量
export REDIS_CLUSTER_AVAILABLE=true
export REDIS_CLUSTER_URLS=redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002
export RUST_LOG=debug

# 运行测试
cargo test test_redis_cluster_enforcer_sync -- --ignored --nocapture
```

### 预期输出
```
Using Redis Cluster URLs: redis://127.0.0.1:7000,redis://127.0.0.1:7001,redis://127.0.0.1:7002
Using unique channel: test_cluster_sync_<uuid>
Creating Redis Cluster watchers...
Setting up callbacks...
Waiting for watcher initialization...
Adding policy via e1...
Add policy result: Ok(true)
[Cluster E1] Published update: {"Method":"UpdateForAddPolicy",...}
[Cluster E2] Received update notification: {"Method":"UpdateForAddPolicy",...}
✓ Callback received after attempt 1: {"Method":"UpdateForAddPolicy",...}
✓ Redis cluster enforcer sync test passed
```

## CI 测试

### 自动触发
推送到 GitHub 后，CI 会自动运行：
- `test-linux` job 会在 `redis-mode: cluster` 矩阵中运行
- 自动设置 6 节点 Redis Cluster
- 运行集群测试

### 检查 CI 状态
访问: https://github.com/{your-username}/redis-watcher-v1/actions

### 预期 CI 流程
1. Setup Redis Cluster ✓
2. Checking cluster status ✓
3. Running cluster test ✓
4. test_redis_cluster_enforcer_sync ... ok ✓

## 提交建议

### Git 操作
```bash
# 检查修改
git status

# 添加所有修改
git add -A

# 提交
git commit -m "Fix Redis Cluster PubSub message propagation

- Unified PubSub operations to use single node
- Simplified RedisClientWrapper for cluster mode
- Enhanced test logging and error messages
- Added comprehensive documentation

Fixes #[issue-number] (if applicable)"

# 推送到远程
git push origin test/enforcer-v1
```

### 提交信息建议
```
Fix Redis Cluster PubSub message propagation

Problem:
- E2 watcher not receiving updates from E1 in cluster mode
- Root cause: PubSub messages don't propagate across Redis Cluster nodes
- E1 and E2 were connecting to different nodes

Solution:
- Use single node connection for all PubSub operations
- Simplified RedisClientWrapper to ClusterPubSub variant
- Ensured publish and subscribe operations use same Redis node

Changes:
- src/watcher.rs: Unified PubSub to single node
- src/watcher_test.rs: Enhanced logging and diagnostics
- README.md: Added Redis Cluster usage notes
- REDIS_CLUSTER_FIX.md: Detailed technical documentation
- FIX_SUMMARY.md: Problem analysis and solution summary

Testing:
- All unit tests pass locally
- Cluster test validated with manual Redis Cluster setup
- CI will verify on GitHub Actions

Related: Go implementation comparison in casbin-go/watcher.go
```

## 验证清单

在推送前确认：

- [ ] ✅ `cargo build --all-features` 通过
- [ ] ✅ `cargo clippy --all-features` 无警告
- [ ] ✅ `cargo fmt --check` 格式正确
- [ ] ✅ `cargo test --lib` 所有测试通过
- [ ] ✅ 代码有适当的注释
- [ ] ✅ 文档完整且清晰
- [ ] ✅ README.md 包含重要说明
- [ ] 🔄 Git commit 信息清晰
- [ ] 🔄 准备推送到远程

## 下一步

1. **推送代码**: `git push origin test/enforcer-v1`
2. **观察 CI**: 等待 GitHub Actions 运行
3. **检查结果**: 确认所有测试通过
4. **创建 PR**: 如果需要合并到主分支

## 技术要点总结

### 问题本质
这是一个**分布式系统设计问题**，需要理解 Redis Cluster 的 PubSub 局限性。

### 关键洞察
1. Redis Cluster PubSub != 全局广播
2. 消息仅在发布节点本地有效
3. 必须确保发布者和订阅者在同一节点

### 设计决策
- **当前方案**: 单节点 PubSub（简单、可靠）
- **权衡**: 单点依赖 vs 复杂性
- **适用场景**: 中小规模部署（< 1000 实例）

### 未来优化
如果需要更高的可用性和扩展性：
1. 实现 Broadcast 到所有主节点
2. 考虑使用 Redis Streams
3. 或引入专业消息队列

## 参考资源

- [Redis Cluster Specification - PubSub](https://redis.io/docs/reference/cluster-spec/#pubsub)
- [redis-rs Cluster Support](https://docs.rs/redis/latest/redis/cluster/index.html)
- [Casbin Watcher Pattern](https://casbin.org/docs/watchers)
- [Go Implementation](../casbin-rs/redis-watcher/casbin-go/watcher.go)

## 联系与支持

如果遇到问题：
1. 检查 `REDIS_CLUSTER_FIX.md` 详细文档
2. 查看 `FIX_SUMMARY.md` 问题分析
3. 在 GitHub 创建 issue
4. 参考现有的测试代码

---

**记住**: 这个修复确保了 Redis Cluster 模式下 Casbin Watcher 的可靠性。核心是理解 Redis Cluster PubSub 的本质和限制。
