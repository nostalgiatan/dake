//! # 高级功能示例
//!
//! 展示 rstream 模块的高级功能，包括超时策略和增强的性能指标

use rstream::{StreamHandle, StreamRegistry, TimeoutStrategy};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== rstream 高级功能示例 ===\n");

    // 示例 1: 使用固定超时策略
    println!("1. 使用固定超时策略:");
    let registry = StreamRegistry::new();
    let entry = registry.get_or_create(1, None).await;
    let mut handle = StreamHandle::new(entry.clone());

    // 设置 5 秒固定超时
    handle.set_timeout_strategy(TimeoutStrategy::Fixed(Duration::from_secs(5)));
    println!("  设置超时策略: 固定 5 秒");

    handle.acquire_read(None).await?;
    println!("  获取读锁成功");

    // 查看指标
    let metrics = handle.get_metrics().await;
    println!("  读锁获取次数: {}", metrics.read_acquires);
    println!("  平均等待时间: {:?}", metrics.avg_read_wait_time());

    handle.release_read().await?;
    println!("  释放读锁成功");

    println!("\n{}", "=".repeat(60));

    // 示例 2: 使用指数退避策略
    println!("\n2. 使用指数退避超时策略:");
    let entry2 = registry.get_or_create(2, None).await;
    let mut handle2 = StreamHandle::new(entry2.clone());

    // 设置指数退避策略：初始 100ms，最多重试 5 次
    handle2.set_timeout_strategy(TimeoutStrategy::ExponentialBackoff {
        initial: Duration::from_millis(100),
        max_retries: 5,
    });
    println!("  设置超时策略: 指数退避（初始 100ms，最多 5 次重试）");

    // 计算各次尝试的超时时间
    let strategy = handle2.timeout_strategy();
    for attempt in 0..6 {
        if let Some(timeout) = strategy.calculate_timeout(attempt, None) {
            println!("    尝试 {}: 超时 {:?}", attempt, timeout);
        } else {
            println!("    尝试 {}: 超过最大重试次数", attempt);
        }
    }

    handle2.acquire_write(None).await?;
    println!("  获取写锁成功");
    handle2.release_write().await?;
    println!("  释放写锁成功");

    println!("\n{}", "=".repeat(60));

    // 示例 3: 使用自适应超时策略
    println!("\n3. 使用自适应超时策略:");
    let entry3 = registry.get_or_create(3, None).await;
    let mut handle3 = StreamHandle::new(entry3.clone());

    // 设置自适应策略：平均等待时间的 2 倍，最小 100ms，最大 10s
    handle3.set_timeout_strategy(TimeoutStrategy::Adaptive {
        multiplier: 2.0,
        min_timeout: Duration::from_millis(100),
        max_timeout: Duration::from_secs(10),
    });
    println!("  设置超时策略: 自适应（2x平均等待时间，100ms-10s）");

    // 执行多次操作以建立历史数据
    for i in 0..5 {
        handle3.acquire_read(None).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
        handle3.release_read().await?;
        println!("    完成第 {} 次读操作", i + 1);
    }

    let metrics = handle3.get_metrics().await;
    println!("\n  性能指标:");
    println!("    读锁获取次数: {}", metrics.read_acquires);
    println!("    平均读锁等待时间: {:?}", metrics.avg_read_wait_time());

    println!("\n{}", "=".repeat(60));

    // 示例 4: 增强的性能指标
    println!("\n4. 增强的性能指标:");
    let entry4 = registry.get_or_create(4, None).await;
    let mut handle4 = StreamHandle::new(entry4.clone());

    // 执行混合操作
    handle4.acquire_read(None).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle4.release_read().await?;

    handle4.acquire_write(None).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    handle4.release_write().await?;

    handle4.acquire_read(None).await?;
    handle4.upgrade(None).await?;
    handle4.downgrade().await?;
    handle4.release_read().await?;

    let metrics = handle4.get_metrics().await;
    println!("  详细性能指标:");
    println!("    读锁获取次数: {}", metrics.read_acquires);
    println!("    写锁获取次数: {}", metrics.write_acquires);
    println!("    升级次数: {}", metrics.upgrades);
    println!("    降级次数: {}", metrics.downgrades);
    println!("    总操作次数: {}", metrics.total_operations());
    println!("    成功率: {:.2}%", metrics.success_rate() * 100.0);
    println!("    平均读锁等待时间: {:?}", metrics.avg_read_wait_time());
    println!("    平均写锁等待时间: {:?}", metrics.avg_write_wait_time());
    println!("    最大等待时间: {:.6}s", metrics.max_wait_seconds);
    println!("    最小等待时间: {:.6}s", metrics.min_wait_seconds);
    println!("    超时失败次数: {}", metrics.timeout_failures);

    println!("\n{}", "=".repeat(60));

    // 示例 5: 多个顺序操作的指标统计
    println!("\n5. 多个顺序操作的指标统计:");
    let entry5 = registry.get_or_create(5, None).await;
    let mut handle5 = StreamHandle::new(Arc::clone(&entry5));

    // 执行多次顺序操作
    for i in 0..5 {
        handle5.acquire_write(None).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
        handle5.release_write().await?;
        println!("    完成第 {} 次写操作", i + 1);
    }

    let metrics = handle5.get_metrics().await;
    println!("\n  顺序写操作后的指标:");
    println!("    写锁获取次数: {}", metrics.write_acquires);
    println!("    总等待时间: {:.6}s", metrics.wait_write_seconds);
    println!("    平均等待时间: {:?}", metrics.avg_write_wait_time());

    println!("\n{}", "=".repeat(60));

    // 示例 6: 无限等待策略
    println!("\n6. 无限等待策略:");
    let entry6 = registry.get_or_create(6, None).await;
    let mut handle6 = StreamHandle::new(entry6.clone());

    handle6.set_timeout_strategy(TimeoutStrategy::Infinite);
    println!("  设置超时策略: 无限等待");

    let strategy = handle6.timeout_strategy();
    match strategy.calculate_timeout(0, None) {
        None => println!("  计算超时: None (无限等待)"),
        Some(timeout) => println!("  计算超时: {:?}", timeout),
    }

    handle6.acquire_read(None).await?;
    println!("  获取读锁成功（使用无限等待）");
    handle6.release_read().await?;

    println!("\n{}", "=".repeat(60));

    println!("\n所有示例执行完成！");
    Ok(())
}
