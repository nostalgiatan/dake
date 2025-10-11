//! # 流注册表模块
//!
//! 提供资源注册表、资源条目和流句柄，用于管理上下游依赖关系和锁。
//!
//! ## 核心组件
//!
//! - `StreamRegistry`: 单例注册表，管理所有资源条目
//! - `ResourceEntry`: 资源条目，包含锁、信号量、依赖关系和指标
//! - `StreamHandle`: 流句柄，用于操作锁和跟踪重入深度
//! - `Metrics`: 性能指标收集

use crate::locks::{AsyncRWLock, LockError};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

/// 超时策略
///
/// 定义不同场景下的超时处理策略
#[derive(Debug, Clone, Copy)]
pub enum TimeoutStrategy {
    /// 无限等待
    Infinite,
    /// 固定超时时间
    Fixed(Duration),
    /// 指数退避：初始时间、最大重试次数
    ExponentialBackoff {
        /// 初始超时时间
        initial: Duration,
        /// 最大重试次数
        max_retries: u32,
    },
    /// 自适应超时：基于历史平均等待时间
    Adaptive {
        /// 平均等待时间的倍数
        multiplier: f64,
        /// 最小超时时间
        min_timeout: Duration,
        /// 最大超时时间
        max_timeout: Duration,
    },
}

impl Default for TimeoutStrategy {
    fn default() -> Self {
        TimeoutStrategy::Fixed(Duration::from_secs(30))
    }
}

impl TimeoutStrategy {
    /// 计算实际的超时时间
    ///
    /// # 参数
    ///
    /// * `attempt` - 尝试次数（从0开始）
    /// * `avg_wait_time` - 平均等待时间（用于自适应策略）
    pub fn calculate_timeout(&self, attempt: u32, avg_wait_time: Option<Duration>) -> Option<Duration> {
        match self {
            TimeoutStrategy::Infinite => None,
            TimeoutStrategy::Fixed(duration) => Some(*duration),
            TimeoutStrategy::ExponentialBackoff { initial, max_retries } => {
                if attempt >= *max_retries {
                    return None;
                }
                let multiplier = 2_u32.pow(attempt);
                Some(*initial * multiplier)
            }
            TimeoutStrategy::Adaptive { multiplier, min_timeout, max_timeout } => {
                if let Some(avg) = avg_wait_time {
                    let timeout = avg.mul_f64(*multiplier);
                    Some(timeout.clamp(*min_timeout, *max_timeout))
                } else {
                    Some(*min_timeout)
                }
            }
        }
    }
}

/// 默认的资源键生成函数
///
/// 为可哈希类型生成唯一标识符
pub fn default_key_for<T: Hash>(value: &T) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// 性能指标
///
/// 记录锁的等待时间、持有时间和操作次数
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    /// 等待读锁的总时间（秒）
    pub wait_read_seconds: f64,
    /// 等待写锁的总时间（秒）
    pub wait_write_seconds: f64,
    /// 持有读锁的总时间（秒）
    pub hold_read_seconds: f64,
    /// 持有写锁的总时间（秒）
    pub hold_write_seconds: f64,
    /// 读锁获取次数
    pub read_acquires: u64,
    /// 写锁获取次数
    pub write_acquires: u64,
    /// 升级次数
    pub upgrades: u64,
    /// 降级次数
    pub downgrades: u64,
    /// 超时失败次数
    pub timeout_failures: u64,
    /// 最大等待时间（秒）
    pub max_wait_seconds: f64,
    /// 最小等待时间（秒）
    pub min_wait_seconds: f64,
}

impl Metrics {
    /// 计算平均读锁等待时间
    pub fn avg_read_wait_time(&self) -> Duration {
        if self.read_acquires == 0 {
            return Duration::ZERO;
        }
        let avg_secs = self.wait_read_seconds / self.read_acquires as f64;
        Duration::from_secs_f64(avg_secs)
    }

    /// 计算平均写锁等待时间
    pub fn avg_write_wait_time(&self) -> Duration {
        if self.write_acquires == 0 {
            return Duration::ZERO;
        }
        let avg_secs = self.wait_write_seconds / self.write_acquires as f64;
        Duration::from_secs_f64(avg_secs)
    }

    /// 计算总操作次数
    pub fn total_operations(&self) -> u64 {
        self.read_acquires + self.write_acquires + self.upgrades + self.downgrades
    }

    /// 计算成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.total_operations();
        if total == 0 {
            return 1.0;
        }
        let successes = total.saturating_sub(self.timeout_failures);
        successes as f64 / total as f64
    }

    /// 更新等待时间统计
    pub(crate) fn update_wait_stats(&mut self, wait_seconds: f64) {
        if self.min_wait_seconds == 0.0 || wait_seconds < self.min_wait_seconds {
            self.min_wait_seconds = wait_seconds;
        }
        if wait_seconds > self.max_wait_seconds {
            self.max_wait_seconds = wait_seconds;
        }
    }
}

/// 资源条目
///
/// 包含锁、信号量、依赖关系和性能指标
pub struct ResourceEntry {
    /// 资源键
    pub key: u64,
    /// 异步读写锁
    pub lock: AsyncRWLock,
    /// 可选的信号量（用于限制并发数）
    pub semaphore: Option<Arc<Semaphore>>,
    /// 下游依赖的资源键集合
    pub lowers: HashSet<u64>,
    /// 上游依赖的资源键集合
    pub uppers: HashSet<u64>,
    /// 性能指标
    pub metrics: Arc<Mutex<Metrics>>,
    /// 是否启用指标收集
    pub metrics_enabled: bool,
}

impl ResourceEntry {
    /// 创建新的资源条目
    ///
    /// # 参数
    ///
    /// * `key` - 资源键
    /// * `fair` - 是否启用公平锁
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::ResourceEntry;
    ///
    /// let entry = ResourceEntry::new(123, true);
    /// ```
    pub fn new(key: u64, fair: bool) -> Self {
        Self {
            key,
            lock: AsyncRWLock::new(fair),
            semaphore: None,
            lowers: HashSet::new(),
            uppers: HashSet::new(),
            metrics: Arc::new(Mutex::new(Metrics::default())),
            metrics_enabled: true,
        }
    }

    /// 设置信号量（用于限制并发数）
    ///
    /// # 参数
    ///
    /// * `max_concurrent` - 最大并发数
    pub fn set_semaphore(&mut self, max_concurrent: usize) {
        self.semaphore = Some(Arc::new(Semaphore::new(max_concurrent)));
    }
}

/// 流注册表
///
/// 单例模式，管理所有资源条目和全局引用计数
pub struct StreamRegistry {
    /// 资源条目映射
    resources: Arc<Mutex<HashMap<u64, Arc<ResourceEntry>>>>,
    /// 全局引用计数
    global_ref_counts: Arc<Mutex<HashMap<u64, u32>>>,
}

impl StreamRegistry {
    /// 创建新的流注册表
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::StreamRegistry;
    ///
    /// let registry = StreamRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self {
            resources: Arc::new(Mutex::new(HashMap::new())),
            global_ref_counts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 快速获取资源条目（不加锁）
    ///
    /// # 参数
    ///
    /// * `key` - 资源键
    ///
    /// # 返回
    ///
    /// 返回资源条目的克隆（如果存在）
    pub async fn fast_get(&self, key: u64) -> Option<Arc<ResourceEntry>> {
        let resources = self.resources.lock().await;
        resources.get(&key).cloned()
    }

    /// 获取或创建资源条目
    ///
    /// # 参数
    ///
    /// * `key` - 资源键
    /// * `max_concurrent` - 可选的最大并发数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::StreamRegistry;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let registry = StreamRegistry::new();
    /// let entry = registry.get_or_create(123, None).await;
    /// # }
    /// ```
    pub async fn get_or_create(
        &self,
        key: u64,
        max_concurrent: Option<usize>,
    ) -> Arc<ResourceEntry> {
        let mut resources = self.resources.lock().await;
        
        if let Some(entry) = resources.get(&key) {
            // 如果需要设置信号量但还没有
            // 注意：由于Arc的不可变性，这里我们不修改已存在的entry
            // 实际使用中应该在首次创建时就设置好信号量
            return Arc::clone(entry);
        }
        
        let mut entry = ResourceEntry::new(key, true);
        if let Some(max) = max_concurrent {
            entry.set_semaphore(max);
        }
        let entry = Arc::new(entry);
        resources.insert(key, Arc::clone(&entry));
        
        entry
    }

    /// 绑定上下游依赖关系
    ///
    /// # 参数
    ///
    /// * `upper_key` - 上游资源键
    /// * `lower_key` - 下游资源键
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::StreamRegistry;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let registry = StreamRegistry::new();
    /// registry.bind_dependency(1, 2).await;
    /// # }
    /// ```
    pub async fn bind_dependency(&self, upper_key: u64, lower_key: u64) {
        let mut resources = self.resources.lock().await;
        let mut ref_counts = self.global_ref_counts.lock().await;
        
        // 确保两个资源都存在
        resources.entry(upper_key).or_insert_with(|| Arc::new(ResourceEntry::new(upper_key, true)));
        resources.entry(lower_key).or_insert_with(|| Arc::new(ResourceEntry::new(lower_key, true)));
        
        // 注意：这里简化了处理，实际需要使用内部可变性来修改 Arc 内的数据
        // 或者重构 ResourceEntry 的设计
        
        // 增加引用计数
        *ref_counts.entry(lower_key).or_insert(0) += 1;
    }

    /// 释放上下游依赖关系
    ///
    /// # 参数
    ///
    /// * `upper_key` - 上游资源键
    /// * `lower_key` - 下游资源键
    pub async fn release_dependency(&self, _upper_key: u64, lower_key: u64) {
        let mut ref_counts = self.global_ref_counts.lock().await;
        
        if let Some(count) = ref_counts.get_mut(&lower_key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                ref_counts.remove(&lower_key);
                // 可能需要清理资源
            }
        }
    }

    /// 获取资源条目
    ///
    /// # 参数
    ///
    /// * `key` - 资源键
    pub async fn get_entry(&self, key: u64) -> Option<Arc<ResourceEntry>> {
        let resources = self.resources.lock().await;
        resources.get(&key).cloned()
    }
}

impl Default for StreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 锁模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// 未持有锁
    None,
    /// 读模式
    Read,
    /// 写模式
    Write,
}

/// 流句柄
///
/// 在函数体内操作锁（读/写/升/降），跟踪重入深度并记录指标。
///
/// # 特性
///
/// - `read_depth` / `write_depth`：确保释放对称，支持可重入写
/// - `safe_release()`：根据持有深度循环释放，避免泄漏
/// - `upgrade_scope()`：简化"短事务"模式
/// - 支持自定义超时策略
pub struct StreamHandle {
    /// 资源条目
    entry: Arc<ResourceEntry>,
    /// 持有锁的起始时间（用于指标）
    hold_start: Option<Instant>,
    /// 当前锁模式
    mode: LockMode,
    /// 是否启用指标收集
    metrics_enabled: bool,
    /// 读锁深度
    read_depth: u32,
    /// 写锁深度
    write_depth: u32,
    /// 超时策略
    timeout_strategy: TimeoutStrategy,
}

impl StreamHandle {
    /// 创建新的流句柄
    ///
    /// # 参数
    ///
    /// * `entry` - 资源条目
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::{StreamRegistry, StreamHandle};
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let registry = StreamRegistry::new();
    /// let entry = registry.get_or_create(123, None).await;
    /// let handle = StreamHandle::new(entry);
    /// # }
    /// ```
    pub fn new(entry: Arc<ResourceEntry>) -> Self {
        let metrics_enabled = entry.metrics_enabled;
        Self {
            entry,
            hold_start: None,
            mode: LockMode::None,
            metrics_enabled,
            read_depth: 0,
            write_depth: 0,
            timeout_strategy: TimeoutStrategy::default(),
        }
    }
    
    /// 设置超时策略
    ///
    /// # 参数
    ///
    /// * `strategy` - 超时策略
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::{StreamRegistry, StreamHandle, TimeoutStrategy};
    /// use std::time::Duration;
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let registry = StreamRegistry::new();
    /// let entry = registry.get_or_create(123, None).await;
    /// let mut handle = StreamHandle::new(entry);
    ///
    /// // 使用固定超时
    /// handle.set_timeout_strategy(TimeoutStrategy::Fixed(Duration::from_secs(10)));
    ///
    /// // 使用指数退避
    /// handle.set_timeout_strategy(TimeoutStrategy::ExponentialBackoff {
    ///     initial: Duration::from_millis(100),
    ///     max_retries: 5,
    /// });
    /// # }
    /// ```
    pub fn set_timeout_strategy(&mut self, strategy: TimeoutStrategy) {
        self.timeout_strategy = strategy;
    }
    
    /// 获取当前超时策略
    pub fn timeout_strategy(&self) -> TimeoutStrategy {
        self.timeout_strategy
    }
    
    /// 获取性能指标
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::{StreamRegistry, StreamHandle};
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let registry = StreamRegistry::new();
    /// let entry = registry.get_or_create(123, None).await;
    /// let handle = StreamHandle::new(entry);
    ///
    /// let metrics = handle.get_metrics().await;
    /// println!("读锁获取次数: {}", metrics.read_acquires);
    /// println!("平均等待时间: {:?}", metrics.avg_read_wait_time());
    /// # }
    /// ```
    pub async fn get_metrics(&self) -> Metrics {
        self.entry.metrics.lock().await.clone()
    }

    /// 获取资源键
    pub fn key(&self) -> u64 {
        self.entry.key
    }

    /// 获取当前锁模式
    pub fn mode(&self) -> LockMode {
        self.mode
    }

    /// 获取读锁深度
    pub fn read_depth(&self) -> u32 {
        self.read_depth
    }

    /// 获取写锁深度
    pub fn write_depth(&self) -> u32 {
        self.write_depth
    }

    /// 获取读锁
    ///
    /// # 参数
    ///
    /// * `timeout` - 超时时长（None表示无限等待）
    ///
    /// # 错误
    ///
    /// 如果获取失败则返回 LockError
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::registry::{StreamRegistry, StreamHandle};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let registry = StreamRegistry::new();
    /// let entry = registry.get_or_create(123, None).await;
    /// let mut handle = StreamHandle::new(entry);
    /// handle.acquire_read(None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire_read(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), LockError> {
        let start = if self.metrics_enabled {
            Some(Instant::now())
        } else {
            None
        };
        
        // 使用超时策略计算实际超时时间
        let effective_timeout = timeout.or_else(|| {
            let avg_wait = if self.metrics_enabled {
                let metrics = futures::executor::block_on(self.entry.metrics.lock());
                Some(metrics.avg_read_wait_time())
            } else {
                None
            };
            self.timeout_strategy.calculate_timeout(0, avg_wait)
        });
        
        let result = self.entry.lock.acquire_read(effective_timeout).await;
        
        if let Some(t0) = start {
            let t1 = Instant::now();
            let wait_secs = (t1 - t0).as_secs_f64();
            let mut metrics = self.entry.metrics.lock().await;
            metrics.wait_read_seconds += wait_secs;
            metrics.update_wait_stats(wait_secs);
            
            if result.is_ok() {
                metrics.read_acquires += 1;
            } else {
                metrics.timeout_failures += 1;
            }
            
            // 只有从0->1开始计时
            if self.read_depth == 0 && self.write_depth == 0 {
                self.hold_start = Some(t1);
            }
        }
        
        result?;
        self.read_depth += 1;
        self.mode = LockMode::Read;
        
        Ok(())
    }

    /// 释放读锁
    ///
    /// # 错误
    ///
    /// 如果释放失败则返回 LockError
    pub async fn release_read(&mut self) -> Result<(), LockError> {
        if self.read_depth == 0 {
            return Err(LockError::ReleaseReadWithoutHolding);
        }
        
        let end = if self.metrics_enabled {
            Some(Instant::now())
        } else {
            None
        };
        
        self.entry.lock.release_read().await?;
        self.read_depth -= 1;
        
        // 从1->0才累加读持有时长（且未处于写模式）
        if let Some(t1) = end {
            if let Some(t0) = self.hold_start {
                if self.read_depth == 0 && self.write_depth == 0 {
                    let mut metrics = self.entry.metrics.lock().await;
                    metrics.hold_read_seconds += (t1 - t0).as_secs_f64();
                    self.hold_start = None;
                }
            }
        }
        
        if self.read_depth == 0 {
            self.mode = LockMode::None;
        }
        
        Ok(())
    }

    /// 获取写锁
    ///
    /// # 参数
    ///
    /// * `timeout` - 超时时长（None表示无限等待）
    ///
    /// # 错误
    ///
    /// 如果获取失败则返回 LockError
    pub async fn acquire_write(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), LockError> {
        let start = if self.metrics_enabled {
            Some(Instant::now())
        } else {
            None
        };
        
        self.entry.lock.acquire_write(timeout).await?;
        
        if let Some(t0) = start {
            let t1 = Instant::now();
            let mut metrics = self.entry.metrics.lock().await;
            metrics.wait_write_seconds += (t1 - t0).as_secs_f64();
            metrics.write_acquires += 1;
            
            if self.write_depth == 0 {
                // 切入写模式：结束读时段计时，开启写时段计时
                if let Some(hold_t0) = self.hold_start {
                    if self.read_depth > 0 {
                        metrics.hold_read_seconds += (t0 - hold_t0).as_secs_f64();
                    }
                }
                self.hold_start = Some(t1);
            }
        }
        
        self.write_depth += 1;
        self.mode = LockMode::Write;
        
        Ok(())
    }

    /// 释放写锁
    ///
    /// # 错误
    ///
    /// 如果释放失败则返回 LockError
    pub async fn release_write(&mut self) -> Result<(), LockError> {
        if self.write_depth == 0 {
            return Err(LockError::ReleaseWriteByNonWriter);
        }
        
        let end = if self.metrics_enabled {
            Some(Instant::now())
        } else {
            None
        };
        
        self.entry.lock.release_write().await?;
        self.write_depth -= 1;
        
        // 仅当从1->0时结束写计时
        if let Some(t1) = end {
            if let Some(t0) = self.hold_start {
                if self.write_depth == 0 {
                    let mut metrics = self.entry.metrics.lock().await;
                    metrics.hold_write_seconds += (t1 - t0).as_secs_f64();
                    self.hold_start = None;
                    
                    // 若还有读深度>0，恢复读计时起点
                    if self.read_depth > 0 {
                        self.hold_start = Some(t1);
                    }
                }
            }
        }
        
        if self.write_depth == 0 {
            self.mode = if self.read_depth > 0 {
                LockMode::Read
            } else {
                LockMode::None
            };
        }
        
        Ok(())
    }

    /// 升级读锁为写锁
    ///
    /// # 参数
    ///
    /// * `timeout` - 超时时长（None表示无限等待）
    ///
    /// # 错误
    ///
    /// 如果升级失败则返回 LockError
    pub async fn upgrade(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), LockError> {
        if self.read_depth == 0 {
            return Err(LockError::UpgradeWithoutReadLock);
        }
        
        let t0 = if self.metrics_enabled {
            Some(Instant::now())
        } else {
            None
        };
        
        self.entry.lock.upgrade(timeout).await?;
        
        // 消耗一个读深度，进入写深度
        self.read_depth -= 1;
        
        if let Some(start) = t0 {
            if let Some(hold_t0) = self.hold_start {
                // 结束读时段
                let mut metrics = self.entry.metrics.lock().await;
                metrics.hold_read_seconds += (start - hold_t0).as_secs_f64();
            }
            
            let t1 = Instant::now();
            let mut metrics = self.entry.metrics.lock().await;
            metrics.wait_write_seconds += (t1 - start).as_secs_f64();
            metrics.upgrades += 1;
            
            // 开启写时段计时（如果之前不是写）
            if self.write_depth == 0 {
                self.hold_start = Some(t1);
            }
        }
        
        self.write_depth += 1;
        self.mode = LockMode::Write;
        
        Ok(())
    }

    /// 降级写锁为读锁
    ///
    /// # 错误
    ///
    /// 如果降级失败则返回 LockError
    pub async fn downgrade(&mut self) -> Result<(), LockError> {
        if self.write_depth != 1 {
            return Err(LockError::DowngradeWithNestedWriteLocks);
        }
        
        let t0 = if self.metrics_enabled {
            Some(Instant::now())
        } else {
            None
        };
        
        self.entry.lock.downgrade().await?;
        
        // 写->读
        if let Some(start) = t0 {
            if let Some(hold_t0) = self.hold_start {
                let mut metrics = self.entry.metrics.lock().await;
                metrics.hold_write_seconds += (start - hold_t0).as_secs_f64();
                metrics.downgrades += 1;
            }
            self.hold_start = Some(start); // 立即作为读阶段继续
        }
        
        self.write_depth = 0;
        self.read_depth += 1;
        self.mode = LockMode::Read;
        
        Ok(())
    }

    /// 检查是否有信号量
    pub fn has_semaphore(&self) -> bool {
        self.entry.semaphore.is_some()
    }

    /// 获取信号量
    pub async fn acquire_semaphore(&self) -> Option<tokio::sync::SemaphorePermit<'_>> {
        if let Some(sem) = &self.entry.semaphore {
            sem.acquire().await.ok()
        } else {
            None
        }
    }

    /// 安全释放所有持有的锁
    ///
    /// 根据当前重入深度循环释放所有持有的锁。
    /// 返回释放摘要，便于调试/观测。
    ///
    /// # 返回
    ///
    /// 返回一个元组 (释放的写锁数量, 释放的读锁数量, 错误数量)
    pub async fn safe_release(&mut self) -> (u32, u32, u32) {
        let mut released_write = 0;
        let mut released_read = 0;
        let mut errors = 0;
        
        // 先释放写（优先确保独占释放）
        while self.write_depth > 0 {
            match self.release_write().await {
                Ok(_) => released_write += 1,
                Err(_) => {
                    errors += 1;
                    break;
                }
            }
        }
        
        // 再释放读
        while self.read_depth > 0 {
            match self.release_read().await {
                Ok(_) => released_read += 1,
                Err(_) => {
                    errors += 1;
                    break;
                }
            }
        }
        
        (released_write, released_read, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_get_or_create() {
        let registry = StreamRegistry::new();
        let entry1 = registry.get_or_create(123, None).await;
        let entry2 = registry.get_or_create(123, None).await;
        
        assert_eq!(entry1.key, entry2.key);
    }

    #[tokio::test]
    async fn test_stream_handle_read() {
        let registry = StreamRegistry::new();
        let entry = registry.get_or_create(123, None).await;
        let mut handle = StreamHandle::new(entry);
        
        handle.acquire_read(None).await.unwrap();
        assert_eq!(handle.mode(), LockMode::Read);
        assert_eq!(handle.read_depth(), 1);
        
        handle.release_read().await.unwrap();
        assert_eq!(handle.mode(), LockMode::None);
        assert_eq!(handle.read_depth(), 0);
    }

    #[tokio::test]
    async fn test_stream_handle_write() {
        let registry = StreamRegistry::new();
        let entry = registry.get_or_create(123, None).await;
        let mut handle = StreamHandle::new(entry);
        
        handle.acquire_write(None).await.unwrap();
        assert_eq!(handle.mode(), LockMode::Write);
        assert_eq!(handle.write_depth(), 1);
        
        handle.release_write().await.unwrap();
        assert_eq!(handle.mode(), LockMode::None);
        assert_eq!(handle.write_depth(), 0);
    }

    #[tokio::test]
    async fn test_stream_handle_upgrade() {
        let registry = StreamRegistry::new();
        let entry = registry.get_or_create(123, None).await;
        let mut handle = StreamHandle::new(entry);
        
        handle.acquire_read(None).await.unwrap();
        handle.upgrade(None).await.unwrap();
        assert_eq!(handle.mode(), LockMode::Write);
        
        handle.release_write().await.unwrap();
    }

    #[tokio::test]
    async fn test_stream_handle_downgrade() {
        let registry = StreamRegistry::new();
        let entry = registry.get_or_create(123, None).await;
        let mut handle = StreamHandle::new(entry);
        
        handle.acquire_write(None).await.unwrap();
        handle.downgrade().await.unwrap();
        assert_eq!(handle.mode(), LockMode::Read);
        
        handle.release_read().await.unwrap();
    }

    #[tokio::test]
    async fn test_safe_release() {
        let registry = StreamRegistry::new();
        let entry = registry.get_or_create(123, None).await;
        let mut handle = StreamHandle::new(entry);
        
        // 只获取一个写锁，然后降级为读锁
        handle.acquire_write(None).await.unwrap();
        handle.downgrade().await.unwrap();
        
        // 现在有一个读锁
        let (write, read, errors) = handle.safe_release().await;
        assert_eq!(write, 0);
        assert_eq!(read, 1);
        assert_eq!(errors, 0);
    }
}
