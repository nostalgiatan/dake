//! # rstream 集成测试
//!
//! 测试完整的使用场景和边界情况

use rstream::locks::{AsyncRWLock, LockError};
use rstream::registry::{StreamRegistry, StreamHandle, LockMode};
use std::sync::Arc;
use std::time::Duration;

/// 测试基本的读写锁操作
#[tokio::test]
async fn test_basic_read_write() {
    let lock = AsyncRWLock::new(true);
    
    // 获取读锁
    lock.acquire_read(None).await.unwrap();
    lock.release_read().await.unwrap();
    
    // 获取写锁
    lock.acquire_write(None).await.unwrap();
    lock.release_write().await.unwrap();
}

/// 测试写锁的可重入性
#[tokio::test]
async fn test_write_reentrant() {
    let lock = AsyncRWLock::new(true);
    
    // 第一次获取写锁
    lock.acquire_write(None).await.unwrap();
    
    // 第二次获取写锁（可重入）
    lock.acquire_write(None).await.unwrap();
    
    // 必须释放两次
    lock.release_write().await.unwrap();
    lock.release_write().await.unwrap();
}

/// 测试读锁升级为写锁
#[tokio::test]
async fn test_read_to_write_upgrade() {
    let lock = AsyncRWLock::new(true);
    
    // 获取读锁
    lock.acquire_read(None).await.unwrap();
    
    // 升级为写锁
    lock.upgrade(None).await.unwrap();
    
    // 释放写锁
    lock.release_write().await.unwrap();
}

/// 测试写锁降级为读锁
#[tokio::test]
async fn test_write_to_read_downgrade() {
    let lock = AsyncRWLock::new(true);
    
    // 获取写锁
    lock.acquire_write(None).await.unwrap();
    
    // 降级为读锁
    lock.downgrade().await.unwrap();
    
    // 释放读锁
    lock.release_read().await.unwrap();
}

/// 测试超时功能
#[tokio::test]
async fn test_timeout() {
    let lock = AsyncRWLock::new(true);
    
    // 先获取写锁
    lock.acquire_write(None).await.unwrap();
    
    // 在另一个任务中尝试获取读锁（会超时）
    let lock_clone = lock.clone();
    let handle = tokio::spawn(async move {
        let result = lock_clone.acquire_read(Some(Duration::from_millis(100))).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, LockError::Timeout));
        }
    });
    
    handle.await.unwrap();
    
    // 释放写锁
    lock.release_write().await.unwrap();
}

/// 测试非阻塞获取
#[tokio::test]
async fn test_try_acquire() {
    let lock = AsyncRWLock::new(true);
    
    // 尝试获取读锁（应该成功）
    assert!(lock.try_acquire_read().await);
    
    // 尝试获取写锁（应该失败，因为有读锁）
    assert!(!lock.try_acquire_write().await);
    
    lock.release_read().await.unwrap();
    
    // 现在尝试获取写锁（应该成功）
    assert!(lock.try_acquire_write().await);
    
    lock.release_write().await.unwrap();
}

/// 测试 StreamRegistry 的基本功能
#[tokio::test]
async fn test_stream_registry() {
    let registry = StreamRegistry::new();
    
    // 获取或创建资源
    let entry1 = registry.get_or_create(100, None).await;
    let entry2 = registry.get_or_create(100, None).await;
    
    // 应该是同一个资源
    assert_eq!(entry1.key, entry2.key);
    
    // 获取不同的资源
    let entry3 = registry.get_or_create(200, None).await;
    assert_ne!(entry1.key, entry3.key);
}

/// 测试 StreamHandle 的完整流程
#[tokio::test]
async fn test_stream_handle_full_cycle() {
    let registry = StreamRegistry::new();
    let entry = registry.get_or_create(300, None).await;
    let mut handle = StreamHandle::new(entry);
    
    // 初始状态
    assert_eq!(handle.mode(), LockMode::None);
    assert_eq!(handle.read_depth(), 0);
    assert_eq!(handle.write_depth(), 0);
    
    // 获取读锁
    handle.acquire_read(None).await.unwrap();
    assert_eq!(handle.mode(), LockMode::Read);
    assert_eq!(handle.read_depth(), 1);
    
    // 升级为写锁
    handle.upgrade(None).await.unwrap();
    assert_eq!(handle.mode(), LockMode::Write);
    assert_eq!(handle.write_depth(), 1);
    assert_eq!(handle.read_depth(), 0);
    
    // 降级为读锁
    handle.downgrade().await.unwrap();
    assert_eq!(handle.mode(), LockMode::Read);
    assert_eq!(handle.read_depth(), 1);
    assert_eq!(handle.write_depth(), 0);
    
    // 释放读锁
    handle.release_read().await.unwrap();
    assert_eq!(handle.mode(), LockMode::None);
}

/// 测试 safe_release
#[tokio::test]
async fn test_safe_release() {
    let registry = StreamRegistry::new();
    let entry = registry.get_or_create(400, None).await;
    let mut handle = StreamHandle::new(entry);
    
    // 获取锁并降级
    handle.acquire_write(None).await.unwrap();
    handle.downgrade().await.unwrap();
    handle.acquire_read(None).await.unwrap();
    
    // 安全释放
    let (write, read, errors) = handle.safe_release().await;
    
    // 应该释放了2个读锁
    assert_eq!(write, 0);
    assert_eq!(read, 2);
    assert_eq!(errors, 0);
    assert_eq!(handle.mode(), LockMode::None);
}

/// 测试错误情况：没有持有锁就释放
#[tokio::test]
async fn test_release_without_holding() {
    let lock = AsyncRWLock::new(true);
    
    // 尝试释放未持有的读锁
    let result = lock.release_read().await;
    assert!(result.is_err());
    
    // 尝试释放未持有的写锁
    let result = lock.release_write().await;
    assert!(result.is_err());
}

/// 测试错误情况：没有读锁就升级
#[tokio::test]
async fn test_upgrade_without_read_lock() {
    let lock = AsyncRWLock::new(true);
    
    // 尝试在没有读锁的情况下升级
    let result = lock.upgrade(None).await;
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, LockError::UpgradeWithoutReadLock));
    }
}

/// 测试错误情况：嵌套写锁时降级
#[tokio::test]
async fn test_downgrade_with_nested_write() {
    let lock = AsyncRWLock::new(true);
    
    // 获取两层写锁
    lock.acquire_write(None).await.unwrap();
    lock.acquire_write(None).await.unwrap();
    
    // 尝试降级（应该失败）
    let result = lock.downgrade().await;
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, LockError::DowngradeWithNestedWriteLocks));
    }
    
    // 清理
    lock.release_write().await.unwrap();
    lock.release_write().await.unwrap();
}

/// 测试性能指标收集
#[tokio::test]
async fn test_metrics_collection() {
    let registry = StreamRegistry::new();
    let entry = registry.get_or_create(500, None).await;
    let mut handle = StreamHandle::new(Arc::clone(&entry));
    
    // 执行一些操作
    handle.acquire_read(None).await.unwrap();
    handle.release_read().await.unwrap();
    
    handle.acquire_write(None).await.unwrap();
    handle.release_write().await.unwrap();
    
    // 检查指标
    let metrics = entry.metrics.lock().await;
    assert_eq!(metrics.read_acquires, 1);
    assert_eq!(metrics.write_acquires, 1);
    assert!(metrics.wait_read_seconds >= 0.0);
    assert!(metrics.wait_write_seconds >= 0.0);
}

/// 测试依赖关系绑定
#[tokio::test]
async fn test_dependency_binding() {
    let registry = StreamRegistry::new();
    
    // 绑定依赖关系
    registry.bind_dependency(1, 2).await;
    
    // 释放依赖关系
    registry.release_dependency(1, 2).await;
}

/// 测试并发场景：多个读者
#[tokio::test]
async fn test_concurrent_readers() {
    let lock = Arc::new(AsyncRWLock::new(true));
    let mut handles = vec![];
    
    for _ in 0..5 {
        let lock_clone = Arc::clone(&lock);
        let handle = tokio::spawn(async move {
            lock_clone.acquire_read(None).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            lock_clone.release_read().await.unwrap();
        });
        handles.push(handle);
    }
    
    for h in handles {
        h.await.unwrap();
    }
}

/// 测试读锁可重入
#[tokio::test]
async fn test_read_reentrant() {
    let lock = AsyncRWLock::new(true);
    
    // 第一次获取读锁
    lock.acquire_read(None).await.unwrap();
    
    // 第二次获取读锁（可重入）
    lock.acquire_read(None).await.unwrap();
    
    // 必须释放两次
    lock.release_read().await.unwrap();
    lock.release_read().await.unwrap();
}
