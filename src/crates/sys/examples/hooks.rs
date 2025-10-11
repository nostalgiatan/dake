//! # 条件钩子示例
//!
//! 演示如何使用条件钩子在系统参数达到阈值时自动触发回调

use sys::{SystemInfo, HookRegistry, HookCondition};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 条件钩子示例 ===\n");

    // 创建系统信息和钩子注册表
    let sys_info = SystemInfo::new().await;
    let mut hook_registry = HookRegistry::new();

    // 使用原子计数器跟踪钩子触发次数
    let cpu_counter = Arc::new(AtomicUsize::new(0));
    let memory_counter = Arc::new(AtomicUsize::new(0));
    let disk_counter = Arc::new(AtomicUsize::new(0));

    // 注册 CPU 使用率钩子（超过 0% 时触发，用于演示）
    let cpu_counter_clone = Arc::clone(&cpu_counter);
    hook_registry.register_cpu_hook(HookCondition::Above(0.0), move || {
        let count = cpu_counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
        println!("  [钩子触发] CPU 使用率钩子被触发 (第 {} 次)", count);
    });

    // 注册内存使用率钩子（超过 10% 时触发）
    let memory_counter_clone = Arc::clone(&memory_counter);
    hook_registry.register_memory_hook(HookCondition::Above(10.0), move || {
        let count = memory_counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
        println!("  [钩子触发] 内存使用率超过 10% (第 {} 次)", count);
    });

    // 注册磁盘使用率钩子（超过 50% 时触发）
    let disk_counter_clone = Arc::clone(&disk_counter);
    hook_registry.register_disk_hook(HookCondition::Above(50.0), move || {
        let count = disk_counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
        println!("  [钩子触发] 磁盘使用率超过 50% (第 {} 次)", count);
    });

    // 注册 CPU 温度钩子（超过 70°C 时触发）
    hook_registry.register_cpu_temp_hook(HookCondition::Above(70.0), || {
        println!("  [钩子触发] ⚠️ CPU 温度过高！");
    });

    // 注册 GPU 温度钩子（如果有 GPU）
    if sys_info.gpu_count() > 0 {
        hook_registry.register_gpu_temp_hook(0, HookCondition::Above(80.0), || {
            println!("  [钩子触发] ⚠️ GPU 0 温度过高！");
        });
    }

    println!("已注册的钩子:");
    println!("  CPU 钩子: {}", hook_registry.cpu_hook_count());
    println!("  内存钩子: {}", hook_registry.memory_hook_count());
    println!("  磁盘钩子: {}", hook_registry.disk_hook_count());
    println!("  CPU 温度钩子: {}", hook_registry.cpu_temp_hook_count());
    println!("  GPU 温度钩子: {}", hook_registry.gpu_temp_hook_count());

    // 检查并触发钩子
    println!("\n开始检查条件...");
    hook_registry.check_and_trigger(&sys_info).await;

    // 显示结果
    println!("\n钩子触发统计:");
    println!("  CPU 钩子触发次数: {}", cpu_counter.load(Ordering::SeqCst));
    println!("  内存钩子触发次数: {}", memory_counter.load(Ordering::SeqCst));
    println!("  磁盘钩子触发次数: {}", disk_counter.load(Ordering::SeqCst));

    // 演示不同的条件类型
    println!("\n=== 不同条件类型示例 ===");

    let mut test_registry = HookRegistry::new();

    // Below 条件
    test_registry.register_cpu_hook(HookCondition::Below(100.0), || {
        println!("  CPU 使用率低于 100%（应该总是触发）");
    });

    // Between 条件
    test_registry.register_memory_hook(HookCondition::Between(0.0, 100.0), || {
        println!("  内存使用率在 0-100% 之间（应该总是触发）");
    });

    // Outside 条件
    test_registry.register_disk_hook(HookCondition::Outside(200.0, 300.0), || {
        println!("  磁盘使用率不在 200-300% 之间（应该总是触发）");
    });

    println!("\n检查不同条件类型...");
    test_registry.check_and_trigger(&sys_info).await;

    println!("\n=== 示例结束 ===");
    Ok(())
}
