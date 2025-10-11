//! # 基础使用示例
//!
//! 演示如何使用 sys 模块获取系统信息

use sys::{SystemInfo, Availability};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 系统信息监控示例 ===\n");

    // 创建系统信息对象
    let sys_info = SystemInfo::new().await;

    // 操作系统信息
    println!("【操作系统信息】");
    println!("  操作系统: {}", sys_info.os_name());
    println!("  系统版本: {}", sys_info.os_version());
    println!("  内核版本: {}", sys_info.kernel_version());
    println!("  主机名: {}", sys_info.host_name());

    // CPU 信息
    println!("\n【CPU 信息】");
    println!("  CPU 核心数: {}", sys_info.cpu_count());
    println!("  CPU 使用率: {:.2}%", sys_info.cpu_usage());
    if let Some(temp) = sys_info.cpu_temperature() {
        println!("  CPU 温度: {:.1}°C", temp);
    }

    // 内存信息
    println!("\n【内存信息】");
    let (used_mem, total_mem) = sys_info.memory_info();
    println!("  已使用内存: {:.2} GB", used_mem as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("  总内存: {:.2} GB", total_mem as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("  内存使用率: {:.2}%", sys_info.memory_usage_percent());

    let (used_swap, total_swap) = sys_info.swap_info();
    println!("  已使用交换空间: {:.2} GB", used_swap as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("  总交换空间: {:.2} GB", total_swap as f64 / (1024.0 * 1024.0 * 1024.0));

    // 磁盘信息
    println!("\n【磁盘信息】");
    let disk_total = sys_info.disk_total();
    let disk_available = sys_info.disk_available();
    println!("  磁盘总大小: {:.2} GB", disk_total as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("  磁盘可用空间: {:.2} GB", disk_available as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("  磁盘使用率: {:.2}%", sys_info.disk_usage_percent());

    // 后端可用性检测
    println!("\n【后端可用性】");
    match sys_info.cuda_available() {
        Availability::Yes => println!("  CUDA: ✓ 可用"),
        Availability::No => println!("  CUDA: ✗ 不可用"),
    }
    match sys_info.vulkan_available() {
        Availability::Yes => println!("  Vulkan: ✓ 可用"),
        Availability::No => println!("  Vulkan: ✗ 不可用"),
    }

    // GPU 信息
    println!("\n【GPU 信息】");
    let gpu_count = sys_info.gpu_count();
    if gpu_count > 0 {
        println!("  GPU 数量: {}", gpu_count);
        for i in 0..gpu_count {
            if let Some(gpu) = sys_info.gpu_info(i) {
                println!("\n  GPU {}:", i);
                println!("    名称: {}", gpu.name);
                println!("    总内存: {:.2} GB", gpu.total_memory as f64 / (1024.0 * 1024.0 * 1024.0));
                println!("    已使用内存: {:.2} GB", gpu.used_memory as f64 / (1024.0 * 1024.0 * 1024.0));
                println!("    内存使用率: {:.2}%", gpu.memory_usage_percent());
                if let Some(temp) = gpu.temperature {
                    println!("    温度: {:.1}°C", temp);
                }
            }
        }
    } else {
        println!("  未检测到 GPU 或 GPU 信息获取功能未启用");
    }

    println!("\n=== 示例结束 ===");
    Ok(())
}
