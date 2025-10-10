# 本地测试发布流程说明

本文档说明如何在本地测试 `redis-watcher-temp` 包的发布流程，包括 tag 和 crate 发布。

## 前置准备

1. 确保你已经有 Cargo 账号并配置了 API token：
   ```bash
   cargo login
   ```

2. 确保项目可以正常编译：
   ```bash
   cargo check
   cargo test
   ```

## 本地测试步骤

### 1. 测试包构建

```bash
# 检查包是否可以正常构建
cargo build --release

# 运行测试
cargo test

# 检查文档是否可以生成
cargo doc --no-deps
```

### 2. 测试包打包

```bash
# 创建包并检查内容
cargo package

# 查看打包后的内容
cargo package --list

# 检查打包后的大小和结构
ls -la target/package/
```

### 3. 测试本地安装

```bash
# 从本地包安装测试
cargo install --path .

# 或者从打包文件安装
cargo install --path target/package/redis-watcher-temp-0.1.0
```

### 4. 发布到 crates.io (测试用临时包名)

**注意：这将真实发布到 crates.io，请确认使用临时包名！**

```bash
# 发布包（使用 --dry-run 先测试）
cargo publish --dry-run

# 如果 dry-run 成功，执行真实发布
cargo publish
```

### 5. 测试 Git Tag 创建

```bash
# 创建并推送 tag
git tag -a v0.1.0 -m "Release version 0.1.0 (test)"
git push origin v0.1.0

# 查看 tag 是否创建成功
git tag -l
git show v0.1.0
```

### 6. 验证发布成功

1. **验证 crates.io 发布**：
   - 访问 https://crates.io/crates/redis-watcher-temp
   - 检查版本是否显示正确

2. **验证包可以被下载和使用**：
   ```bash
   # 在新目录创建测试项目
   mkdir test-project && cd test-project
   cargo init
   
   # 添加依赖到 Cargo.toml
   echo 'redis-watcher-temp = "0.1.0"' >> Cargo.toml
   
   # 测试是否可以正常下载和编译
   cargo check
   ```

3. **验证文档**：
   - 访问 https://docs.rs/redis-watcher-temp/0.1.0
   - 检查文档是否正确生成

### 7. 清理测试包（可选）

如果测试完成后想删除测试包：

```bash
# 删除 git tag
git tag -d v0.1.0
git push origin --delete v0.1.0
```

**注意**：crates.io 上发布的包无法删除，只能 yank（标记为不推荐使用）：
```bash
cargo yank --vers 0.1.0 redis-watcher-temp
```

## 准备正式发布

测试成功后，准备正式发布时需要：

1. 将包名改回 `redis-watcher`
2. 更新所有相关 URL 指向官方仓库
3. 确保版本号符合语义化版本规范
4. 准备完整的 CHANGELOG
5. 确保所有测试通过
6. 创建 PR 到官方仓库

## 注意事项

- 测试包名 `redis-watcher-temp` 将永久占用该名称
- crates.io 不允许删除已发布的包，只能 yank
- 确保在测试环境中验证所有功能
- 考虑在测试分支进行，避免污染主分支历史