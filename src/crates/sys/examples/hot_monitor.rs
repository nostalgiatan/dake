//! # 热监控示例
//!
//! 演示如何使用异步热监控模式定期刷新系统信息并触发钩子

use sys::{SystemInfo, HotMonitor, HookRegistry, HookCondition};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 异步热监控示例 ===\n");

    // 创建系统信息和钩子注册表
    let sys_info = SystemInfo::new().await;
    let mut hook_registry = HookRegistry::new();

    // 注册钩子：CPU 使用率超过 0% 时输出（用于演示）
    hook_registry.register_cpu_hook(HookCondition::Above(0.0), || {
        println!("  [监控] CPU 使用率被监控");
    });

    // 注册钩子：内存使用率超过 50% 时警告
    hook_registry.register_memory_hook(HookCondition::Above(50.0), || {
        println!("  [警告] ⚠️ 内存使用率超过 50%");
    });

    // 注册钩子：磁盘使用率超过 80% 时警告
    hook_registry.register_disk_hook(HookCondition::Above(80.0), || {
        println!("  [警告] ⚠️ 磁盘使用率超过 80%");
    });

    // 创建热监控器，每秒刷新一次
    println!("启动热监控器（每 1 秒刷新一次）...\n");
    let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
    let handle = monitor.start().await;

    // 运行监控并定期显示信息
    for i in 1..=5 {
        println!("--- 第 {} 秒 ---", i);
        
        // 获取当前系统信息
        {
            let sys = handle.get_sys_info().await;
            println!("  CPU 使用率: {:.2}%", sys.cpu_usage());
            println!("  内存使用率: {:.2}%", sys.memory_usage_percent());
            println!("  磁盘使用率: {:.2}%", sys.disk_usage_percent());
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    println!("\n演示动态添加钩子...");
    
    // 动态添加新钩子
    {
        let mut registry = handle.get_hook_registry_mut().await;
        registry.register_cpu_hook(HookCondition::Above(20.0), || {
            println!("  [新钩子] CPU 使用率超过 20%");
        });
    }

    println!("已添加新的 CPU 钩子（超过 20% 触发）\n");

    // 继续运行监控
    for i in 6..=8 {
        println!("--- 第 {} 秒 ---", i);
        
        {
            let sys = handle.get_sys_info().await;
            println!("  CPU 使用率: {:.2}%", sys.cpu_usage());
            println!("  内存使用率: {:.2}%", sys.memory_usage_percent());
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // 停止监控
    println!("\n停止热监控器...");
    handle.stop().await;
    println!("监控器已停止");

    println!("\n=== 示例结束 ===");
    Ok(())
}
