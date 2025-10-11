//! # 流处理宏示例
//!
//! 演示如何使用 `lower_stream` 和 `upper_stream` 宏标记流处理函数

use rstream::{
    lower_stream, upper_stream,
    registry::{StreamRegistry, StreamHandle},
    LockError,
};

/// 下游流处理函数
///
/// 使用 `lower_stream` 宏标记，表明这是一个读取数据的下游流处理函数
#[lower_stream]
async fn read_downstream_data(handle: &mut StreamHandle) -> Result<Vec<u8>, LockError> {
    println!("下游流: 开始读取数据");
    
    // 显式获取读锁
    handle.acquire_read(None).await?;
    println!("下游流: 已获取读锁");
    
    // 模拟读取数据
    let data = vec![1, 2, 3, 4, 5];
    println!("下游流: 读取数据: {:?}", data);
    
    // 显式释放读锁
    handle.release_read().await?;
    println!("下游流: 已释放读锁");
    
    Ok(data)
}

/// 上游流处理函数
///
/// 使用 `upper_stream` 宏标记，表明这是一个修改数据的上游流处理函数
#[upper_stream]
async fn write_upstream_data(handle: &mut StreamHandle, data: Vec<u8>) -> Result<(), LockError> {
    println!("上游流: 开始写入数据");
    
    // 显式获取写锁
    handle.acquire_write(None).await?;
    println!("上游流: 已获取写锁");
    
    // 模拟写入数据
    println!("上游流: 写入数据: {:?}", data);
    
    // 显式释放写锁
    handle.release_write().await?;
    println!("上游流: 已释放写锁");
    
    Ok(())
}

/// 带锁升级的上游流处理函数
///
/// 演示如何在上游流中先读取后修改
#[upper_stream]
async fn read_modify_write(handle: &mut StreamHandle) -> Result<Vec<u8>, LockError> {
    println!("读-修改-写: 开始处理");
    
    // 先获取读锁读取数据
    handle.acquire_read(None).await?;
    println!("读-修改-写: 已获取读锁");
    
    // 读取当前数据
    let mut data = vec![1, 2, 3];
    println!("读-修改-写: 当前数据: {:?}", data);
    
    // 升级到写锁进行修改
    handle.upgrade(None).await?;
    println!("读-修改-写: 已升级到写锁");
    
    // 修改数据
    data.push(4);
    println!("读-修改-写: 修改后数据: {:?}", data);
    
    // 降级回读锁
    handle.downgrade().await?;
    println!("读-修改-写: 已降级回读锁");
    
    // 可以继续读取
    println!("读-修改-写: 验证数据: {:?}", data);
    
    // 释放读锁
    handle.release_read().await?;
    println!("读-修改-写: 已释放读锁");
    
    Ok(data)
}

#[tokio::main]
async fn main() -> Result<(), LockError> {
    println!("=== 流处理宏示例 ===\n");
    
    // 创建流注册表
    let registry = StreamRegistry::new();
    
    // 注册一个资源 (resource_id: 1, capacity: None)
    let entry = registry.get_or_create(1, None).await;
    let mut handle = StreamHandle::new(entry);
    
    println!("场景1: 下游流读取");
    println!("------------------");
    let data = read_downstream_data(&mut handle).await?;
    println!("读取到数据: {:?}\n", data);
    
    println!("场景2: 上游流写入");
    println!("------------------");
    write_upstream_data(&mut handle, vec![10, 20, 30]).await?;
    println!();
    
    println!("场景3: 读-修改-写操作");
    println!("------------------");
    let modified_data = read_modify_write(&mut handle).await?;
    println!("最终数据: {:?}\n", modified_data);
    
    println!("=== 示例完成 ===");
    
    Ok(())
}
