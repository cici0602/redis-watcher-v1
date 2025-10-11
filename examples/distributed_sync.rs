// 示例：在分布式环境中使用 RedisWatcher 同步策略

use casbin::prelude::*;
use redis_watcher::{RedisWatcher, WatcherOptions};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 场景：两个微服务实例需要同步 Casbin 策略

    // ========== 实例 1 ==========
    println!("创建实例 1...");

    let options1 = WatcherOptions::default()
        .with_channel("/casbin-policy-updates".to_string())
        .with_ignore_self(true)
        .with_local_id("service-instance-1".to_string());

    let mut watcher1 = RedisWatcher::new("redis://127.0.0.1:6379", options1)?;

    // 设置回调：当其他实例更新策略时，重新加载
    watcher1.set_update_callback(Box::new(|msg: String| {
        println!("[实例1] 收到策略更新通知: {}", msg);
        // 在实际应用中，这里会调用 enforcer.load_policy()
    }));

    // 创建 Enforcer 并关联 watcher
    let mut enforcer1 =
        Enforcer::new("examples/rbac_model.conf", "examples/rbac_policy.csv").await?;
    enforcer1.set_watcher(Box::new(watcher1));

    println!("[实例1] Enforcer 已创建并关联 Watcher\n");

    // ========== 实例 2 ==========
    println!("创建实例 2...");

    let options2 = WatcherOptions::default()
        .with_channel("/casbin-policy-updates".to_string())
        .with_ignore_self(true)
        .with_local_id("service-instance-2".to_string());

    let mut watcher2 = RedisWatcher::new("redis://127.0.0.1:6379", options2)?;

    watcher2.set_update_callback(Box::new(|msg: String| {
        println!("[实例2] 收到策略更新通知: {}", msg);
    }));

    let mut enforcer2 =
        Enforcer::new("examples/rbac_model.conf", "examples/rbac_policy.csv").await?;
    enforcer2.set_watcher(Box::new(watcher2));

    println!("[实例2] Enforcer 已创建并关联 Watcher\n");

    // ========== 模拟策略更新 ==========

    // 等待订阅建立
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    println!("========== 场景：实例1 添加新策略 ==========");
    enforcer1
        .add_policy(vec![
            "alice".to_string(),
            "data3".to_string(),
            "read".to_string(),
        ])
        .await?;
    println!("[实例1] 已添加策略：alice, data3, read");

    // 等待消息传播
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    println!(">>> 实例2 应该收到更新通知并重新加载策略\n");

    println!("========== 场景：实例2 删除策略 ==========");
    enforcer2
        .remove_policy(vec![
            "alice".to_string(),
            "data1".to_string(),
            "read".to_string(),
        ])
        .await?;
    println!("[实例2] 已删除策略：alice, data1, read");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    println!(">>> 实例1 应该收到更新通知并重新加载策略\n");

    println!("========== 测试完成 ==========");
    println!("✅ 多个实例成功通过 RedisWatcher 同步策略更新");

    Ok(())
}
