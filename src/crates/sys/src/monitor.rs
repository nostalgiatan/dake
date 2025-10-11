//! # 异步热监控模块
//!
//! 提供高性能、低内存占用的异步监控功能，定期更新系统信息并触发钩子。
//!
//! ## 示例
//!
//! ```no_run
//! use sys::{SystemInfo, HotMonitor};
//! use sys::hooks::{HookRegistry, HookCondition};
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let sys_info = SystemInfo::new().await;
//! let mut hook_registry = HookRegistry::new();
//!
//! hook_registry.register_cpu_hook(HookCondition::Above(80.0), || {
//!     println!("CPU 使用率过高!");
//! });
//!
//! let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
//! let handle = monitor.start().await;
//!
//! // 运行一段时间后停止
//! tokio::time::sleep(Duration::from_secs(10)).await;
//! handle.stop().await;
//! # Ok(())
//! # }
//! ```

use crate::SystemInfo;
use crate::hooks::HookRegistry;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// 热监控器
///
/// 定期刷新系统信息并触发钩子。
///
/// # 特性
///
/// - **异步执行**: 使用 tokio 异步运行时
/// - **低内存占用**: 最小化内存分配
/// - **可配置间隔**: 自定义监控间隔
pub struct HotMonitor {
    /// 系统信息（共享）
    sys_info: Arc<RwLock<SystemInfo>>,
    /// 钩子注册表（共享）
    hook_registry: Arc<RwLock<HookRegistry>>,
    /// 监控间隔
    interval: Duration,
}

impl HotMonitor {
    /// 创建新的热监控器
    ///
    /// # 参数
    ///
    /// - `sys_info`: 系统信息对象
    /// - `hook_registry`: 钩子注册表
    /// - `interval`: 监控间隔
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use sys::{SystemInfo, HotMonitor};
    /// use sys::hooks::HookRegistry;
    /// use std::time::Duration;
    ///
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let hook_registry = HookRegistry::new();
    /// let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
    /// # }
    /// ```
    pub fn new(sys_info: SystemInfo, hook_registry: HookRegistry, interval: Duration) -> Self {
        Self {
            sys_info: Arc::new(RwLock::new(sys_info)),
            hook_registry: Arc::new(RwLock::new(hook_registry)),
            interval,
        }
    }

    /// 启动监控
    ///
    /// 返回监控句柄，可用于停止监控。
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use sys::{SystemInfo, HotMonitor};
    /// use sys::hooks::HookRegistry;
    /// use std::time::Duration;
    ///
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let hook_registry = HookRegistry::new();
    /// let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
    /// let handle = monitor.start().await;
    /// # }
    /// ```
    pub async fn start(self) -> MonitorHandle {
        let sys_info = Arc::clone(&self.sys_info);
        let hook_registry = Arc::clone(&self.hook_registry);
        let interval = self.interval;

        let running = Arc::new(RwLock::new(true));
        let running_clone = Arc::clone(&running);

        let task = tokio::spawn(async move {
            while *running_clone.read().await {
                // 刷新系统信息
                {
                    let mut sys = sys_info.write().await;
                    sys.refresh();
                }

                // 检查并触发钩子
                {
                    let sys = sys_info.read().await;
                    let registry = hook_registry.read().await;
                    registry.check_and_trigger(&sys).await;
                }

                // 等待间隔时间
                tokio::time::sleep(interval).await;
            }
        });

        MonitorHandle {
            task,
            running,
            sys_info: self.sys_info,
            hook_registry: self.hook_registry,
        }
    }
}

/// 监控句柄
///
/// 用于控制和查询监控状态。
pub struct MonitorHandle {
    /// 监控任务句柄
    task: JoinHandle<()>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
    /// 系统信息（共享）
    sys_info: Arc<RwLock<SystemInfo>>,
    /// 钩子注册表（共享）
    hook_registry: Arc<RwLock<HookRegistry>>,
}

impl MonitorHandle {
    /// 停止监控
    ///
    /// 等待监控任务完成。
    ///
    /// # 示例
    ///
    /// ```no_run
    /// # use sys::{SystemInfo, HotMonitor};
    /// # use sys::hooks::HookRegistry;
    /// # use std::time::Duration;
    /// # async fn example() {
    /// # let sys_info = SystemInfo::new().await;
    /// # let hook_registry = HookRegistry::new();
    /// # let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
    /// let handle = monitor.start().await;
    /// handle.stop().await;
    /// # }
    /// ```
    pub async fn stop(self) {
        // 设置运行状态为 false
        {
            let mut running = self.running.write().await;
            *running = false;
        }

        // 等待任务完成
        let _ = self.task.await;
    }

    /// 检查监控是否正在运行
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// 获取系统信息的只读引用
    ///
    /// # 示例
    ///
    /// ```no_run
    /// # use sys::{SystemInfo, HotMonitor};
    /// # use sys::hooks::HookRegistry;
    /// # use std::time::Duration;
    /// # async fn example() {
    /// # let sys_info = SystemInfo::new().await;
    /// # let hook_registry = HookRegistry::new();
    /// # let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
    /// let handle = monitor.start().await;
    /// let sys_info = handle.get_sys_info().await;
    /// let cpu_usage = sys_info.cpu_usage();
    /// # }
    /// ```
    pub async fn get_sys_info(&self) -> tokio::sync::RwLockReadGuard<'_, SystemInfo> {
        self.sys_info.read().await
    }

    /// 获取钩子注册表的只读引用
    pub async fn get_hook_registry(&self) -> tokio::sync::RwLockReadGuard<'_, HookRegistry> {
        self.hook_registry.read().await
    }

    /// 获取钩子注册表的可写引用
    ///
    /// 用于在监控运行时动态添加或删除钩子。
    ///
    /// # 示例
    ///
    /// ```no_run
    /// # use sys::{SystemInfo, HotMonitor};
    /// # use sys::hooks::{HookRegistry, HookCondition};
    /// # use std::time::Duration;
    /// # async fn example() {
    /// # let sys_info = SystemInfo::new().await;
    /// # let hook_registry = HookRegistry::new();
    /// # let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_secs(1));
    /// let handle = monitor.start().await;
    /// let mut registry = handle.get_hook_registry_mut().await;
    /// registry.register_cpu_hook(HookCondition::Above(90.0), || {
    ///     println!("新增钩子被触发!");
    /// });
    /// # }
    /// ```
    pub async fn get_hook_registry_mut(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, HookRegistry> {
        self.hook_registry.write().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookCondition;

    #[tokio::test]
    async fn test_hot_monitor_creation() {
        let sys_info = SystemInfo::new().await;
        let hook_registry = HookRegistry::new();
        let _monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_hot_monitor_start_stop() {
        let sys_info = SystemInfo::new().await;
        let hook_registry = HookRegistry::new();
        let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_millis(100));

        let handle = monitor.start().await;
        assert!(handle.is_running().await);

        // 等待一小段时间
        tokio::time::sleep(Duration::from_millis(250)).await;

        handle.stop().await;
    }

    #[tokio::test]
    async fn test_hot_monitor_with_hook() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let sys_info = SystemInfo::new().await;
        let mut hook_registry = HookRegistry::new();

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // 注册一个总是触发的钩子
        hook_registry.register_cpu_hook(HookCondition::Above(0.0), move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_millis(50));
        let handle = monitor.start().await;

        // 运行一段时间
        tokio::time::sleep(Duration::from_millis(200)).await;

        handle.stop().await;

        // 验证钩子被多次触发
        assert!(counter.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn test_hot_monitor_get_sys_info() {
        let sys_info = SystemInfo::new().await;
        let hook_registry = HookRegistry::new();
        let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_millis(100));

        let handle = monitor.start().await;

        // 获取系统信息（在作用域内释放借用）
        {
            let sys = handle.get_sys_info().await;
            let _cpu_usage = sys.cpu_usage();
        }

        handle.stop().await;
    }

    #[tokio::test]
    async fn test_hot_monitor_dynamic_hook() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let sys_info = SystemInfo::new().await;
        let hook_registry = HookRegistry::new();
        let monitor = HotMonitor::new(sys_info, hook_registry, Duration::from_millis(50));

        let handle = monitor.start().await;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // 动态添加钩子
        {
            let mut registry = handle.get_hook_registry_mut().await;
            registry.register_cpu_hook(HookCondition::Above(0.0), move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        // 运行一段时间
        tokio::time::sleep(Duration::from_millis(200)).await;

        handle.stop().await;

        // 验证钩子被触发
        assert!(counter.load(Ordering::SeqCst) >= 2);
    }
}
