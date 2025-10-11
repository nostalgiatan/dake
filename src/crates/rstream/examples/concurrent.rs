//! # 并发访问示例
//!
//! 演示 rstream 的并发访问功能，包括：
//! - 读写锁的基本使用
//! - 锁的升级和降级
//! - 性能指标收集

use rstream::registry::{StreamRegistry, StreamHandle};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 并发访问示例 ===\n");

    // 创建注册表和资源
    let registry = Arc::new(StreamRegistry::new());
    let entry = registry.get_or_create(789, None).await;

    // 示例 1: 顺序访问 - 读取
    println!("示例 1: 顺序读取访问");
    {
        for i in 1..=3 {
            let mut h = StreamHandle::new(Arc::clone(&entry));
            h.acquire_read(None).await?;
            println!("   ✓ 读者 {} 获取读锁", i);
            sleep(Duration::from_millis(10)).await;
            println!("   ✓ 读者 {} 完成读取", i);
            h.release_read().await?;
        }
        println!("   所有读者完成\n");
    }

    // 示例 2: 顺序访问 - 写入
    println!("示例 2: 顺序写入访问");
    {
        let mut h = StreamHandle::new(Arc::clone(&entry));
        h.acquire_write(None).await?;
        println!("   ✓ 写者获取写锁");
        sleep(Duration::from_millis(50)).await;
        println!("   ✓ 写者完成写入");
        h.release_write().await?;
        println!("   写者完成\n");
    }

    // 示例 3: 读-升级-写-降级-读模式
    println!("示例 3: 完整的读-写循环");
    {
        let mut h = StreamHandle::new(Arc::clone(&entry));
        
        // 读取阶段
        println!("   1. 获取读锁");
        h.acquire_read(None).await?;
        println!("      ✓ 模式: {:?}", h.mode());
        sleep(Duration::from_millis(10)).await;
        
        // 升级到写入阶段
        println!("   2. 升级到写锁");
        h.upgrade(None).await?;
        println!("      ✓ 模式: {:?}", h.mode());
        sleep(Duration::from_millis(10)).await;
        
        // 降级回读取阶段
        println!("   3. 降级到读锁");
        h.downgrade().await?;
        println!("      ✓ 模式: {:?}", h.mode());
        sleep(Duration::from_millis(10)).await;
        
        // 释放
        println!("   4. 释放读锁");
        h.release_read().await?;
        println!("      ✓ 模式: {:?}\n", h.mode());
    }

    // 示例 4: 带超时的锁获取
    println!("示例 4: 带超时的锁获取");
    {
        let mut h = StreamHandle::new(Arc::clone(&entry));
        
        // 使用超时获取读锁
        match h.acquire_read(Some(Duration::from_secs(5))).await {
            Ok(_) => {
                println!("   ✓ 成功获取读锁（带5秒超时）");
                h.release_read().await?;
            }
            Err(e) => {
                println!("   ✗ 获取读锁失败: {:?}", e);
            }
        }
        println!();
    }

    // 查看最终指标
    println!("最终性能指标:");
    let metrics = entry.metrics.lock().await;
    println!("- 读锁获取次数: {}", metrics.read_acquires);
    println!("- 写锁获取次数: {}", metrics.write_acquires);
    println!("- 升级次数: {}", metrics.upgrades);
    println!("- 降级次数: {}", metrics.downgrades);
    println!("- 总等待读锁时间: {:.6} 秒", metrics.wait_read_seconds);
    println!("- 总等待写锁时间: {:.6} 秒", metrics.wait_write_seconds);
    println!("- 总持有读锁时间: {:.6} 秒", metrics.hold_read_seconds);
    println!("- 总持有写锁时间: {:.6} 秒", metrics.hold_write_seconds);

    println!("\n=== 示例完成 ===");
    Ok(())
}
