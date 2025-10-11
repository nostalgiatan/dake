//! # 系统信息监控模块
//!
//! 提供跨平台的系统信息获取和监控能力，包括 CPU、内存、磁盘等信息。
//! 支持条件钩子和异步热监控模式。
//!
//! ## 特性
//!
//! - **零隐式转换**: 所有操作都是显式的，确保最高性能
//! - **异步监控**: 提供低内存占用的异步热监控模式
//! - **条件钩子**: 支持属性值达到阈值时自动触发回调
//! - **完整错误处理**: 使用 error 模块统一处理错误
//!
//! ## 使用示例
//!
//! ```no_run
//! use sys::SystemInfo;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 获取系统信息
//! let mut sys_info = SystemInfo::new().await;
//!
//! // 获取 CPU 使用率
//! let cpu_usage = sys_info.cpu_usage();
//! println!("CPU 使用率: {:.2}%", cpu_usage);
//!
//! // 获取内存信息
//! let (used, total) = sys_info.memory_info();
//! println!("内存: {} / {} GB", used / (1024 * 1024 * 1024), total / (1024 * 1024 * 1024));
//! # Ok(())
//! # }
//! ```

use error::ErrorInfo;
use sysinfo::{System, Disks, Components};

pub mod monitor;
pub mod hooks;

pub use monitor::HotMonitor;
pub use hooks::{Hook, HookCondition, HookRegistry};

/// 系统信息监控结果类型
pub type SysResult<T> = std::result::Result<T, ErrorInfo>;

/// 系统信息
///
/// 封装系统的各种信息，包括 CPU、内存、磁盘等。
///
/// # 示例
///
/// ```no_run
/// use sys::SystemInfo;
///
/// # async fn example() {
/// let sys_info = SystemInfo::new().await;
/// println!("操作系统: {}", sys_info.os_name());
/// # }
/// ```
pub struct SystemInfo {
    /// sysinfo 系统对象
    system: System,
}

impl SystemInfo {
    /// 创建新的系统信息对象
    ///
    /// 初始化并刷新系统信息。
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use sys::SystemInfo;
    ///
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// # }
    /// ```
    pub async fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            system,
        }
    }

    /// 刷新系统信息
    ///
    /// 更新所有系统信息，包括 CPU、内存等。
    pub fn refresh(&mut self) {
        self.system.refresh_all();
    }

    /// 获取操作系统名称
    pub fn os_name(&self) -> String {
        System::name().unwrap_or_else(|| String::from("Unknown"))
    }

    /// 获取操作系统版本
    pub fn os_version(&self) -> String {
        System::os_version().unwrap_or_else(|| String::from("Unknown"))
    }

    /// 获取内核版本
    pub fn kernel_version(&self) -> String {
        System::kernel_version().unwrap_or_else(|| String::from("Unknown"))
    }

    /// 获取主机名
    pub fn host_name(&self) -> String {
        System::host_name().unwrap_or_else(|| String::from("Unknown"))
    }

    /// 获取 CPU 使用率（百分比）
    pub fn cpu_usage(&self) -> f32 {
        self.system.global_cpu_usage()
    }

    /// 获取 CPU 核心数
    pub fn cpu_count(&self) -> usize {
        self.system.cpus().len()
    }

    /// 获取内存信息（已使用，总量）
    ///
    /// # 返回值
    ///
    /// 返回元组 `(used_bytes, total_bytes)`
    pub fn memory_info(&self) -> (u64, u64) {
        let used = self.system.used_memory();
        let total = self.system.total_memory();
        (used, total)
    }

    /// 获取内存使用率（百分比）
    pub fn memory_usage_percent(&self) -> f32 {
        let (used, total) = self.memory_info();
        if total == 0 {
            0.0
        } else {
            (used as f64 / total as f64 * 100.0) as f32
        }
    }

    /// 获取交换内存信息（已使用，总量）
    ///
    /// # 返回值
    ///
    /// 返回元组 `(used_bytes, total_bytes)`
    pub fn swap_info(&self) -> (u64, u64) {
        let used = self.system.used_swap();
        let total = self.system.total_swap();
        (used, total)
    }

    /// 获取磁盘总大小（字节）
    pub fn disk_total(&self) -> u64 {
        let disks = Disks::new_with_refreshed_list();
        disks.iter().map(|disk| disk.total_space()).sum()
    }

    /// 获取磁盘可用空间（字节）
    pub fn disk_available(&self) -> u64 {
        let disks = Disks::new_with_refreshed_list();
        disks.iter().map(|disk| disk.available_space()).sum()
    }

    /// 获取磁盘使用率（百分比）
    pub fn disk_usage_percent(&self) -> f32 {
        let total = self.disk_total();
        let available = self.disk_available();
        if total == 0 {
            0.0
        } else {
            let used = total - available;
            (used as f64 / total as f64 * 100.0) as f32
        }
    }

    /// 获取 CPU 温度
    ///
    /// # 返回值
    ///
    /// 如果系统支持，返回 CPU 温度（摄氏度）。
    pub fn cpu_temperature(&self) -> Option<f32> {
        // sysinfo 在某些平台支持温度读取
        let components = Components::new_with_refreshed_list();
        components.iter()
            .find(|component| component.label().contains("CPU") || component.label().contains("cpu"))
            .map(|component| component.temperature())
            .flatten()
    }

    /// 获取物理 CPU 核心数
    ///
    /// 返回系统的物理 CPU 核心数（不包括超线程）。
    ///
    /// # 示例
    ///
    /// ```
    /// # use sys::SystemInfo;
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let physical_cores = sys_info.physical_cpu_count();
    /// println!("物理核心数: {}", physical_cores);
    /// # }
    /// ```
    pub fn physical_cpu_count(&self) -> usize {
        System::physical_core_count().unwrap_or_else(|| {
            // 如果无法获取物理核心数，使用逻辑核心数的一半作为估计
            self.cpu_count() / 2
        })
    }

    /// 获取推荐的线程数
    ///
    /// 根据系统负载和核心数，返回推荐的并行线程数。
    ///
    /// # 示例
    ///
    /// ```
    /// # use sys::SystemInfo;
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let threads = sys_info.recommended_thread_count();
    /// println!("推荐线程数: {}", threads);
    /// # }
    /// ```
    pub fn recommended_thread_count(&self) -> usize {
        let physical_cores = self.physical_cpu_count();
        let cpu_usage = self.cpu_usage();
        
        // 如果 CPU 使用率低于 50%，使用全部物理核心
        // 否则，根据负载降低线程数
        if cpu_usage < 50.0 {
            physical_cores
        } else if cpu_usage < 80.0 {
            (physical_cores * 3 / 4).max(1)
        } else {
            (physical_cores / 2).max(1)
        }
    }

    /// 获取 L1 缓存大小（字节）
    ///
    /// 返回每个核心的 L1 数据缓存大小。
    /// 由于 sysinfo 不直接提供缓存信息，使用基于架构的估计值。
    ///
    /// # 示例
    ///
    /// ```
    /// # use sys::SystemInfo;
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let l1_cache = sys_info.l1_cache_size();
    /// println!("L1 缓存: {} KB", l1_cache / 1024);
    /// # }
    /// ```
    pub fn l1_cache_size(&self) -> usize {
        // L1 数据缓存通常为 32KB（现代 CPU）
        32 * 1024
    }

    /// 获取 L2 缓存大小（字节）
    ///
    /// 返回每个核心的 L2 缓存大小。
    ///
    /// # 示例
    ///
    /// ```
    /// # use sys::SystemInfo;
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let l2_cache = sys_info.l2_cache_size();
    /// println!("L2 缓存: {} KB", l2_cache / 1024);
    /// # }
    /// ```
    pub fn l2_cache_size(&self) -> usize {
        // L2 缓存通常为 256KB 或 512KB（现代 CPU）
        256 * 1024
    }

    /// 获取 L3 缓存大小（字节）
    ///
    /// 返回共享的 L3 缓存大小。
    ///
    /// # 示例
    ///
    /// ```
    /// # use sys::SystemInfo;
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let l3_cache = sys_info.l3_cache_size();
    /// println!("L3 缓存: {} MB", l3_cache / (1024 * 1024));
    /// # }
    /// ```
    pub fn l3_cache_size(&self) -> usize {
        // L3 缓存通常为每个物理核心 2-4 MB
        // 使用保守估计：每核心 2MB
        self.physical_cpu_count() * 2 * 1024 * 1024
    }

    /// 获取缓存行大小（字节）
    ///
    /// 返回 CPU 缓存行的大小，用于对齐优化。
    ///
    /// # 示例
    ///
    /// ```
    /// # use sys::SystemInfo;
    /// # async fn example() {
    /// let sys_info = SystemInfo::new().await;
    /// let cache_line = sys_info.cache_line_size();
    /// println!("缓存行大小: {} 字节", cache_line);
    /// # }
    /// ```
    pub fn cache_line_size(&self) -> usize {
        // 现代 CPU 的缓存行大小通常为 64 字节
        64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_info_creation() {
        let sys_info = SystemInfo::new().await;
        
        // 基本信息应该总是可用
        assert!(!sys_info.os_name().is_empty());
        assert!(sys_info.cpu_count() > 0);
        
        let (_, total_mem) = sys_info.memory_info();
        assert!(total_mem > 0);
    }

    #[tokio::test]
    async fn test_memory_usage_percent() {
        let sys_info = SystemInfo::new().await;
        let usage = sys_info.memory_usage_percent();
        
        // 内存使用率应该在 0-100 之间
        assert!(usage >= 0.0 && usage <= 100.0);
    }

    #[tokio::test]
    async fn test_disk_info() {
        let sys_info = SystemInfo::new().await;
        let total = sys_info.disk_total();
        let available = sys_info.disk_available();
        
        // 磁盘空间应该大于 0
        assert!(total > 0);
        assert!(available <= total);
    }

    #[tokio::test]
    async fn test_refresh() {
        let mut sys_info = SystemInfo::new().await;
        let _usage_before = sys_info.cpu_usage();
        
        // 刷新系统信息
        sys_info.refresh();
        
        let usage_after = sys_info.cpu_usage();
        
        // CPU 使用率应该是有效值
        assert!(usage_after >= 0.0);
    }

    #[tokio::test]
    async fn test_physical_cpu_count() {
        let sys_info = SystemInfo::new().await;
        let physical_cores = sys_info.physical_cpu_count();
        let logical_cores = sys_info.cpu_count();
        
        // 物理核心数应该小于等于逻辑核心数
        assert!(physical_cores > 0);
        assert!(physical_cores <= logical_cores);
    }

    #[tokio::test]
    async fn test_recommended_thread_count() {
        let sys_info = SystemInfo::new().await;
        let threads = sys_info.recommended_thread_count();
        let physical_cores = sys_info.physical_cpu_count();
        
        // 推荐线程数应该在合理范围内
        assert!(threads > 0);
        assert!(threads <= physical_cores);
    }

    #[tokio::test]
    async fn test_cache_sizes() {
        let sys_info = SystemInfo::new().await;
        
        let l1 = sys_info.l1_cache_size();
        let l2 = sys_info.l2_cache_size();
        let l3 = sys_info.l3_cache_size();
        
        // 缓存大小应该符合预期（L1 < L2 < L3）
        assert_eq!(l1, 32 * 1024); // 32KB
        assert_eq!(l2, 256 * 1024); // 256KB
        assert!(l3 > 0);
        assert!(l1 < l2);
        assert!(l2 < l3);
    }

    #[tokio::test]
    async fn test_cache_line_size() {
        let sys_info = SystemInfo::new().await;
        let cache_line = sys_info.cache_line_size();
        
        // 缓存行大小应该为 64 字节
        assert_eq!(cache_line, 64);
    }
}
