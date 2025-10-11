//! # 锁升级和降级示例
//!
//! 演示 rstream 的锁升级和降级功能，包括：
//! - 读锁升级为写锁
//! - 写锁降级为读锁
//! - 升级作用域

use rstream::registry::{StreamRegistry, StreamHandle};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 锁升级和降级示例 ===\n");

    // 创建注册表和资源
    let registry = StreamRegistry::new();
    let entry = registry.get_or_create(456, None).await;

    // 示例 1: 读锁升级为写锁
    println!("示例 1: 读锁升级为写锁");
    {
        let mut handle = StreamHandle::new(Arc::clone(&entry));
        
        // 获取读锁
        println!("1. 获取读锁");
        handle.acquire_read(None).await?;
        println!("   ✓ 当前模式: {:?}, 读深度: {}", handle.mode(), handle.read_depth());
        
        // 模拟读操作
        println!("2. 执行读操作");
        println!("   ✓ 正在读取数据...");
        
        // 升级为写锁
        println!("3. 升级为写锁");
        handle.upgrade(None).await?;
        println!("   ✓ 当前模式: {:?}, 写深度: {}", handle.mode(), handle.write_depth());
        
        // 模拟写操作
        println!("4. 执行写操作");
        println!("   ✓ 正在修改数据...");
        
        // 释放写锁
        handle.release_write().await?;
        println!("5. 释放写锁完成\n");
    }

    // 示例 2: 写锁降级为读锁
    println!("示例 2: 写锁降级为读锁");
    {
        let mut handle = StreamHandle::new(Arc::clone(&entry));
        
        // 获取写锁
        println!("1. 获取写锁");
        handle.acquire_write(None).await?;
        println!("   ✓ 当前模式: {:?}, 写深度: {}", handle.mode(), handle.write_depth());
        
        // 模拟写操作
        println!("2. 执行写操作");
        println!("   ✓ 正在修改数据...");
        
        // 降级为读锁
        println!("3. 降级为读锁");
        handle.downgrade().await?;
        println!("   ✓ 当前模式: {:?}, 读深度: {}", handle.mode(), handle.read_depth());
        
        // 模拟读操作
        println!("4. 执行读操作");
        println!("   ✓ 正在读取数据...");
        
        // 释放读锁
        handle.release_read().await?;
        println!("5. 释放读锁完成\n");
    }

    // 示例 3: 写锁的可重入
    println!("示例 3: 写锁的可重入");
    {
        let mut handle = StreamHandle::new(Arc::clone(&entry));
        
        // 第一次获取写锁
        println!("1. 第一次获取写锁");
        handle.acquire_write(None).await?;
        println!("   ✓ 写深度: {}", handle.write_depth());
        
        // 第二次获取写锁（可重入）
        println!("2. 第二次获取写锁（可重入）");
        handle.acquire_write(None).await?;
        println!("   ✓ 写深度: {}", handle.write_depth());
        
        // 释放第一次写锁
        println!("3. 释放第一次写锁");
        handle.release_write().await?;
        println!("   ✓ 写深度: {}", handle.write_depth());
        
        // 释放第二次写锁
        println!("4. 释放第二次写锁");
        handle.release_write().await?;
        println!("   ✓ 写深度: {}\n", handle.write_depth());
    }

    // 示例 4: 安全释放
    println!("示例 4: 安全释放");
    {
        let mut handle = StreamHandle::new(Arc::clone(&entry));
        
        // 获取多层锁 - 正确的方式：先获取写锁，然后降级为读锁
        println!("1. 获取写锁然后降级为读锁");
        handle.acquire_write(None).await?;
        println!("   ✓ 写深度: {}", handle.write_depth());
        handle.downgrade().await?;
        println!("   ✓ 读深度: {}", handle.read_depth());
        
        // 安全释放所有锁
        println!("2. 调用 safe_release()");
        let (write_released, read_released, errors) = handle.safe_release().await;
        println!("   ✓ 释放了 {} 个写锁", write_released);
        println!("   ✓ 释放了 {} 个读锁", read_released);
        println!("   ✓ 错误数: {}", errors);
        println!("   ✓ 当前模式: {:?}\n", handle.mode());
    }

    // 查看最终指标
    println!("最终性能指标:");
    let metrics = entry.metrics.lock().await;
    println!("- 读锁获取次数: {}", metrics.read_acquires);
    println!("- 写锁获取次数: {}", metrics.write_acquires);
    println!("- 升级次数: {}", metrics.upgrades);
    println!("- 降级次数: {}", metrics.downgrades);
    println!("- 持有读锁时间: {:.6} 秒", metrics.hold_read_seconds);
    println!("- 持有写锁时间: {:.6} 秒", metrics.hold_write_seconds);

    println!("\n=== 示例完成 ===");
    Ok(())
}
