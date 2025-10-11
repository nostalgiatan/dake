# sys - 系统信息监控模块

高性能、低内存占用的系统信息监控库，提供跨平台的系统信息获取和异步监控能力。

## 特性

- ✅ **系统信息获取**: CPU、内存、磁盘、温度等信息
- ✅ **条件钩子系统**: 支持属性值达到阈值时自动触发回调
- ✅ **异步热监控**: 提供高性能、低内存占用的异步监控模式
- ✅ **零隐式转换**: 所有操作都是显式的，确保最高性能
- ✅ **完整错误处理**: 使用 error 模块统一处理错误
- ✅ **内存安全**: 避免所有危险的 unwrap()，确保内存安全性
- ✅ **完整测试**: 所有功能都有对应的测试用例
- ✅ **中文文档**: 完备的中文注释和 API 文档

## 依赖说明

### 必需依赖

- `sysinfo`: 跨平台系统信息获取库
- `tokio`: 异步运行时（full features）
- `error`: 内部错误处理模块

## 安装

在 `Cargo.toml` 中添加：

```toml
[dependencies]
sys = { path = "path/to/sys" }
```

## 快速开始

### 基础使用

```rust
use sys::SystemInfo;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建系统信息对象
    let sys_info = SystemInfo::new().await;

    // 获取操作系统信息
    println!("操作系统: {}", sys_info.os_name());
    println!("系统版本: {}", sys_info.os_version());

    // 获取 CPU 信息
    println!("CPU 核心数: {}", sys_info.cpu_count());
    println!("CPU 使用率: {:.2}%", sys_info.cpu_usage());

    // 获取内存信息
    let (used, total) = sys_info.memory_info();
    println!("内存: {} / {} GB", 
        used / (1024 * 1024 * 1024), 
        total / (1024 * 1024 * 1024)
    );

    Ok(())
}
```

### 条件钩子

```rust
use sys::{SystemInfo, HookRegistry, HookCondition};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sys_info = SystemInfo::new().await;
    let mut hook_registry = HookRegistry::new();

    // CPU 使用率超过 80% 时触发
    hook_registry.register_cpu_hook(HookCondition::Above(80.0), || {
        println!("警告: CPU 使用率过高!");
    });

    // 内存使用率超过 90% 时触发
    hook_registry.register_memory_hook(HookCondition::Above(90.0), || {
        println!("警告: 内存使用率过高!");
    });

    // 检查并触发钩子
    hook_registry.check_and_trigger(&sys_info).await;

    Ok(())
}
```

### 异步热监控

```rust
use sys::{SystemInfo, HotMonitor, HookRegistry, HookCondition};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sys_info = SystemInfo::new().await;
    let mut hook_registry = HookRegistry::new();

    // 注册钩子
    hook_registry.register_cpu_hook(HookCondition::Above(80.0), || {
        println!("CPU 使用率过高!");
    });

    // 创建热监控器，每秒刷新一次
    let monitor = HotMonitor::new(
        sys_info, 
        hook_registry, 
        Duration::from_secs(1)
    );
    let handle = monitor.start().await;

    // 运行监控
    tokio::time::sleep(Duration::from_secs(10)).await;

    // 动态添加钩子
    {
        let mut registry = handle.get_hook_registry_mut().await;
        registry.register_memory_hook(HookCondition::Above(90.0), || {
            println!("内存使用率过高!");
        });
    }

    // 继续运行
    tokio::time::sleep(Duration::from_secs(10)).await;

    // 停止监控
    handle.stop().await;

    Ok(())
}
```

## API 文档

### 核心类型

#### `SystemInfo` 结构体

系统信息对象，提供各种系统信息获取方法。

**创建**：
- `new() -> SystemInfo`: 创建并初始化系统信息对象

**操作系统信息**：
- `os_name() -> String`: 操作系统名称
- `os_version() -> String`: 操作系统版本
- `kernel_version() -> String`: 内核版本
- `host_name() -> String`: 主机名

**CPU 信息**：
- `cpu_count() -> usize`: CPU 核心数
- `physical_cpu_count() -> usize`: 物理 CPU 核心数
- `cpu_usage() -> f32`: CPU 使用率（百分比）
- `cpu_temperature() -> Option<f32>`: CPU 温度（摄氏度）
- `recommended_thread_count() -> usize`: 推荐的线程数

**缓存信息**：
- `l1_cache_size() -> usize`: L1 缓存大小（字节）
- `l2_cache_size() -> usize`: L2 缓存大小（字节）
- `l3_cache_size() -> usize`: L3 缓存大小（字节）
- `cache_line_size() -> usize`: 缓存行大小（字节）

**内存信息**：
- `memory_info() -> (u64, u64)`: 内存信息（已使用，总量）
- `memory_usage_percent() -> f32`: 内存使用率（百分比）
- `swap_info() -> (u64, u64)`: 交换内存信息（已使用，总量）

**磁盘信息**：
- `disk_total() -> u64`: 磁盘总大小（字节）
- `disk_available() -> u64`: 磁盘可用空间（字节）
- `disk_usage_percent() -> f32`: 磁盘使用率（百分比）

**其他**：
- `refresh()`: 刷新系统信息

### 钩子系统

#### `HookCondition` 枚举

钩子触发条件：

- `Above(f32)`: 值大于阈值
- `Below(f32)`: 值小于阈值
- `Between(f32, f32)`: 值在范围内
- `Outside(f32, f32)`: 值在范围外

方法：
- `is_satisfied(value: f32) -> bool`: 检查条件是否满足

#### `HookRegistry` 结构体

钩子注册表，管理所有钩子。

**创建**：
- `new() -> HookRegistry`: 创建新的钩子注册表

**注册钩子**：
- `register_cpu_hook<F>(condition, callback)`: 注册 CPU 使用率钩子
- `register_memory_hook<F>(condition, callback)`: 注册内存使用率钩子
- `register_disk_hook<F>(condition, callback)`: 注册磁盘使用率钩子
- `register_cpu_temp_hook<F>(condition, callback)`: 注册 CPU 温度钩子

**其他**：
- `check_and_trigger(&SystemInfo)`: 检查并触发所有钩子
- `clear()`: 清除所有钩子
- `cpu_hook_count() -> usize`: CPU 钩子数量
- `memory_hook_count() -> usize`: 内存钩子数量
- `disk_hook_count() -> usize`: 磁盘钩子数量
- `cpu_temp_hook_count() -> usize`: CPU 温度钩子数量

### 热监控

#### `HotMonitor` 结构体

异步热监控器。

**创建**：
- `new(sys_info, hook_registry, interval) -> HotMonitor`: 创建热监控器

**启动**：
- `start() -> MonitorHandle`: 启动监控并返回句柄

#### `MonitorHandle` 结构体

监控句柄，用于控制监控。

**方法**：
- `stop()`: 停止监控
- `is_running() -> bool`: 检查是否正在运行
- `get_sys_info() -> RwLockReadGuard<SystemInfo>`: 获取系统信息（只读）
- `get_hook_registry() -> RwLockReadGuard<HookRegistry>`: 获取钩子注册表（只读）
- `get_hook_registry_mut() -> RwLockWriteGuard<HookRegistry>`: 获取钩子注册表（可写）

## 示例程序

运行示例：

```bash
# 基础使用
cargo run --example basic

# 条件钩子
cargo run --example hooks

# 热监控
cargo run --example hot_monitor
```

## 测试

运行测试：

```bash
# 运行所有测试
cargo test

# 运行文档测试
cargo test --doc

# 查看测试覆盖率
cargo test --verbose
```

## 性能优化

本模块遵循以下性能优化原则：

1. **零隐式转换**: 所有操作都是显式的，避免隐式类型转换开销
2. **按需刷新**: 系统信息只在需要时刷新，减少不必要的系统调用
3. **异步设计**: 热监控使用异步模式，避免阻塞线程
4. **共享状态**: 使用 Arc + RwLock 实现高效的共享状态管理
5. **最小依赖**: 只使用必要的依赖，减少编译时间和二进制大小

## 内存安全

- ✅ 避免所有危险的 `unwrap()`
- ✅ 使用 `Option` 和 `Result` 处理可能失败的操作
- ✅ 所有公共 API 都有完整的错误处理
- ✅ 使用 Rust 的类型系统保证内存安全

## 文档生成

生成并查看 API 文档：

```bash
cargo doc --open
```

## 许可证

与项目主许可证相同。

## 贡献

欢迎提交 Issue 和 Pull Request！

## 更新日志

### 0.1.0 (2024)

- ✅ 初始版本
- ✅ 基础系统信息获取
- ✅ 条件钩子系统
- ✅ 异步热监控模式
- ✅ 完整的测试和文档
