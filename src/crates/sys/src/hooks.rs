//! # 条件钩子模块
//!
//! 提供条件触发机制，当系统属性达到特定阈值时自动执行回调函数。
//!
//! ## 示例
//!
//! ```no_run
//! use sys::hooks::{HookRegistry, HookCondition};
//! use sys::SystemInfo;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut registry = HookRegistry::new();
//! let sys_info = SystemInfo::new().await;
//!
//! // CPU 使用率超过 80% 时触发
//! registry.register_cpu_hook(HookCondition::Above(80.0), || {
//!     println!("警告: CPU 使用率过高!");
//! });
//!
//! // 检查并触发钩子
//! registry.check_and_trigger(&sys_info).await;
//! # Ok(())
//! # }
//! ```

use crate::SystemInfo;

/// 钩子条件
///
/// 定义触发钩子的条件。
#[derive(Debug, Clone, Copy)]
pub enum HookCondition {
    /// 值大于阈值
    Above(f32),
    /// 值小于阈值
    Below(f32),
    /// 值在范围内
    Between(f32, f32),
    /// 值在范围外
    Outside(f32, f32),
}

impl HookCondition {
    /// 检查条件是否满足
    ///
    /// # 参数
    ///
    /// - `value`: 当前值
    ///
    /// # 返回值
    ///
    /// 如果条件满足返回 `true`。
    pub fn is_satisfied(&self, value: f32) -> bool {
        match self {
            HookCondition::Above(threshold) => value > *threshold,
            HookCondition::Below(threshold) => value < *threshold,
            HookCondition::Between(min, max) => value >= *min && value <= *max,
            HookCondition::Outside(min, max) => value < *min || value > *max,
        }
    }
}

/// 钩子类型
type HookFn = Box<dyn Fn() + Send + Sync>;

/// 钩子
///
/// 封装条件和回调函数。
pub struct Hook {
    /// 条件
    condition: HookCondition,
    /// 回调函数
    callback: HookFn,
}

impl Hook {
    /// 创建新的钩子
    ///
    /// # 参数
    ///
    /// - `condition`: 触发条件
    /// - `callback`: 回调函数
    pub fn new<F>(condition: HookCondition, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            condition,
            callback: Box::new(callback),
        }
    }

    /// 检查条件并执行回调
    ///
    /// # 参数
    ///
    /// - `value`: 当前值
    ///
    /// # 返回值
    ///
    /// 如果条件满足并执行了回调，返回 `true`。
    pub fn check_and_execute(&self, value: f32) -> bool {
        if self.condition.is_satisfied(value) {
            (self.callback)();
            true
        } else {
            false
        }
    }
}

/// 钩子注册表
///
/// 管理所有钩子并提供检查和触发功能。
///
/// # 示例
///
/// ```no_run
/// use sys::hooks::{HookRegistry, HookCondition};
///
/// let mut registry = HookRegistry::new();
/// registry.register_cpu_hook(HookCondition::Above(90.0), || {
///     println!("CPU 使用率过高!");
/// });
/// ```
pub struct HookRegistry {
    /// CPU 使用率钩子
    cpu_hooks: Vec<Hook>,
    /// 内存使用率钩子
    memory_hooks: Vec<Hook>,
    /// 磁盘使用率钩子
    disk_hooks: Vec<Hook>,
    /// CPU 温度钩子
    cpu_temp_hooks: Vec<Hook>,
}

impl HookRegistry {
    /// 创建新的钩子注册表
    pub fn new() -> Self {
        Self {
            cpu_hooks: Vec::new(),
            memory_hooks: Vec::new(),
            disk_hooks: Vec::new(),
            cpu_temp_hooks: Vec::new(),
        }
    }

    /// 注册 CPU 使用率钩子
    ///
    /// # 参数
    ///
    /// - `condition`: 触发条件
    /// - `callback`: 回调函数
    pub fn register_cpu_hook<F>(&mut self, condition: HookCondition, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.cpu_hooks.push(Hook::new(condition, callback));
    }

    /// 注册内存使用率钩子
    ///
    /// # 参数
    ///
    /// - `condition`: 触发条件
    /// - `callback`: 回调函数
    pub fn register_memory_hook<F>(&mut self, condition: HookCondition, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.memory_hooks.push(Hook::new(condition, callback));
    }

    /// 注册磁盘使用率钩子
    ///
    /// # 参数
    ///
    /// - `condition`: 触发条件
    /// - `callback`: 回调函数
    pub fn register_disk_hook<F>(&mut self, condition: HookCondition, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.disk_hooks.push(Hook::new(condition, callback));
    }

    /// 注册 CPU 温度钩子
    ///
    /// # 参数
    ///
    /// - `condition`: 触发条件
    /// - `callback`: 回调函数
    pub fn register_cpu_temp_hook<F>(&mut self, condition: HookCondition, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.cpu_temp_hooks.push(Hook::new(condition, callback));
    }

    /// 检查并触发所有钩子
    ///
    /// # 参数
    ///
    /// - `sys_info`: 系统信息
    pub async fn check_and_trigger(&self, sys_info: &SystemInfo) {
        // 检查 CPU 使用率钩子
        let cpu_usage = sys_info.cpu_usage();
        for hook in &self.cpu_hooks {
            hook.check_and_execute(cpu_usage);
        }

        // 检查内存使用率钩子
        let memory_usage = sys_info.memory_usage_percent();
        for hook in &self.memory_hooks {
            hook.check_and_execute(memory_usage);
        }

        // 检查磁盘使用率钩子
        let disk_usage = sys_info.disk_usage_percent();
        for hook in &self.disk_hooks {
            hook.check_and_execute(disk_usage);
        }

        // 检查 CPU 温度钩子
        if let Some(temp) = sys_info.cpu_temperature() {
            for hook in &self.cpu_temp_hooks {
                hook.check_and_execute(temp);
            }
        }
    }

    /// 清除所有钩子
    pub fn clear(&mut self) {
        self.cpu_hooks.clear();
        self.memory_hooks.clear();
        self.disk_hooks.clear();
        self.cpu_temp_hooks.clear();
    }

    /// 获取 CPU 钩子数量
    pub fn cpu_hook_count(&self) -> usize {
        self.cpu_hooks.len()
    }

    /// 获取内存钩子数量
    pub fn memory_hook_count(&self) -> usize {
        self.memory_hooks.len()
    }

    /// 获取磁盘钩子数量
    pub fn disk_hook_count(&self) -> usize {
        self.disk_hooks.len()
    }

    /// 获取 CPU 温度钩子数量
    pub fn cpu_temp_hook_count(&self) -> usize {
        self.cpu_temp_hooks.len()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_condition_above() {
        let condition = HookCondition::Above(50.0);
        assert!(condition.is_satisfied(60.0));
        assert!(!condition.is_satisfied(40.0));
        assert!(!condition.is_satisfied(50.0));
    }

    #[test]
    fn test_hook_condition_below() {
        let condition = HookCondition::Below(50.0);
        assert!(condition.is_satisfied(40.0));
        assert!(!condition.is_satisfied(60.0));
        assert!(!condition.is_satisfied(50.0));
    }

    #[test]
    fn test_hook_condition_between() {
        let condition = HookCondition::Between(30.0, 70.0);
        assert!(condition.is_satisfied(50.0));
        assert!(condition.is_satisfied(30.0));
        assert!(condition.is_satisfied(70.0));
        assert!(!condition.is_satisfied(20.0));
        assert!(!condition.is_satisfied(80.0));
    }

    #[test]
    fn test_hook_condition_outside() {
        let condition = HookCondition::Outside(30.0, 70.0);
        assert!(condition.is_satisfied(20.0));
        assert!(condition.is_satisfied(80.0));
        assert!(!condition.is_satisfied(50.0));
        assert!(!condition.is_satisfied(30.0));
        assert!(!condition.is_satisfied(70.0));
    }

    #[test]
    fn test_hook_execution() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let triggered = Arc::new(AtomicBool::new(false));
        let triggered_clone = Arc::clone(&triggered);

        let hook = Hook::new(HookCondition::Above(50.0), move || {
            triggered_clone.store(true, Ordering::SeqCst);
        });

        // 条件不满足，不触发
        assert!(!hook.check_and_execute(40.0));
        assert!(!triggered.load(Ordering::SeqCst));

        // 条件满足，触发
        assert!(hook.check_and_execute(60.0));
        assert!(triggered.load(Ordering::SeqCst));
    }

    #[test]
    fn test_hook_registry_creation() {
        let registry = HookRegistry::new();
        assert_eq!(registry.cpu_hook_count(), 0);
        assert_eq!(registry.memory_hook_count(), 0);
        assert_eq!(registry.disk_hook_count(), 0);
    }

    #[test]
    fn test_hook_registry_register() {
        let mut registry = HookRegistry::new();

        registry.register_cpu_hook(HookCondition::Above(80.0), || {});
        assert_eq!(registry.cpu_hook_count(), 1);

        registry.register_memory_hook(HookCondition::Above(90.0), || {});
        assert_eq!(registry.memory_hook_count(), 1);

        registry.register_disk_hook(HookCondition::Above(95.0), || {});
        assert_eq!(registry.disk_hook_count(), 1);
    }

    #[test]
    fn test_hook_registry_clear() {
        let mut registry = HookRegistry::new();

        registry.register_cpu_hook(HookCondition::Above(80.0), || {});
        registry.register_memory_hook(HookCondition::Above(90.0), || {});

        assert_eq!(registry.cpu_hook_count(), 1);
        assert_eq!(registry.memory_hook_count(), 1);

        registry.clear();

        assert_eq!(registry.cpu_hook_count(), 0);
        assert_eq!(registry.memory_hook_count(), 0);
    }

    #[tokio::test]
    async fn test_hook_registry_check_and_trigger() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let sys_info = SystemInfo::new().await;
        let mut registry = HookRegistry::new();

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // 注册一个总是触发的钩子
        registry.register_cpu_hook(HookCondition::Above(0.0), move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        // 触发钩子
        registry.check_and_trigger(&sys_info).await;

        // 验证钩子被触发
        assert!(counter.load(Ordering::SeqCst) > 0);
    }
}
