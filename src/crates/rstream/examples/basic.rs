//! # 基本使用示例
//!
//! 演示 rstream 的基本功能，包括：
//! - 创建注册表
//! - 获取资源条目
//! - 使用流句柄操作锁
//! - 读写锁的获取和释放

use rstream::registry::{StreamRegistry, StreamHandle};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== rstream 基本使用示例 ===\n");

    // 创建注册表
    println!("1. 创建注册表");
    let registry = StreamRegistry::new();
    println!("   ✓ 注册表创建成功\n");

    // 获取或创建资源
    println!("2. 获取资源条目");
    let key = 123;
    let entry = registry.get_or_create(key, None).await;
    println!("   ✓ 资源条目 {} 创建成功\n", key);

    // 创建流句柄
    println!("3. 创建流句柄");
    let mut handle = StreamHandle::new(Arc::clone(&entry));
    println!("   ✓ 流句柄创建成功\n");

    // 获取读锁
    println!("4. 获取读锁");
    handle.acquire_read(None).await?;
    println!("   ✓ 读锁获取成功");
    println!("   - 当前模式: {:?}", handle.mode());
    println!("   - 读锁深度: {}\n", handle.read_depth());

    // 执行读操作
    println!("5. 执行读操作");
    println!("   ✓ 模拟读取数据...\n");

    // 释放读锁
    println!("6. 释放读锁");
    handle.release_read().await?;
    println!("   ✓ 读锁释放成功");
    println!("   - 当前模式: {:?}\n", handle.mode());

    // 获取写锁
    println!("7. 获取写锁");
    handle.acquire_write(None).await?;
    println!("   ✓ 写锁获取成功");
    println!("   - 当前模式: {:?}", handle.mode());
    println!("   - 写锁深度: {}\n", handle.write_depth());

    // 执行写操作
    println!("8. 执行写操作");
    println!("   ✓ 模拟写入数据...\n");

    // 释放写锁
    println!("9. 释放写锁");
    handle.release_write().await?;
    println!("   ✓ 写锁释放成功");
    println!("   - 当前模式: {:?}\n", handle.mode());

    // 查看指标
    println!("10. 查看性能指标");
    let metrics = entry.metrics.lock().await;
    println!("   - 读锁获取次数: {}", metrics.read_acquires);
    println!("   - 写锁获取次数: {}", metrics.write_acquires);
    println!("   - 等待读锁时间: {:.6} 秒", metrics.wait_read_seconds);
    println!("   - 等待写锁时间: {:.6} 秒", metrics.wait_write_seconds);
    println!("   - 持有读锁时间: {:.6} 秒", metrics.hold_read_seconds);
    println!("   - 持有写锁时间: {:.6} 秒\n", metrics.hold_write_seconds);

    println!("=== 示例完成 ===");
    Ok(())
}
