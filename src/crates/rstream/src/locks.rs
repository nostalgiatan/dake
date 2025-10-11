//! # 异步读写锁模块
//!
//! 提供公平且支持升级/降级的异步读写锁，带写锁可重入功能。
//!
//! ## 特性
//!
//! - **公平性**: 默认有写等待者时阻止新读者进入（可配置）
//! - **升级**: 将当前任务的一个读锁升级为写锁（原子等待，超时回滚）
//! - **降级**: 写锁降为读锁；若写锁为可重入（>1），则不允许降级
//! - **写锁可重入**: 同一任务多次 acquire_write 必须配对 release_write
//! - **超时支持**: 所有获取操作都支持超时

use error::Error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tokio::time::timeout;

/// 超时错误
#[derive(Debug, Error)]
pub enum LockError {
    /// 获取锁超时
    #[error("获取锁操作超时")]
    Timeout,
    
    /// 释放读锁失败：当前任务未持有读锁
    #[error("释放读锁失败：当前任务未持有读锁")]
    ReleaseReadWithoutHolding,
    
    /// 释放写锁失败：当前任务不是写锁持有者
    #[error("释放写锁失败：当前任务不是写锁持有者")]
    ReleaseWriteByNonWriter,
    
    /// 升级失败：当前任务未持有读锁
    #[error("升级失败：当前任务未持有读锁")]
    UpgradeWithoutReadLock,
    
    /// 降级失败：当前任务不是写锁持有者
    #[error("降级失败：当前任务不是写锁持有者")]
    DowngradeByNonWriter,
    
    /// 降级失败：持有嵌套写锁时不能降级
    #[error("降级失败：持有嵌套写锁时不能降级，请先释放写锁")]
    DowngradeWithNestedWriteLocks,
}

impl std::error::Error for LockError {}

/// 任务ID类型
type TaskId = usize;

/// 全局任务ID计数器
static TASK_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// 获取当前任务ID
fn current_task_id() -> TaskId {
    // 使用thread_local存储任务ID，确保每个线程有唯一ID
    thread_local! {
        static TASK_ID: std::cell::Cell<Option<TaskId>> = const { std::cell::Cell::new(None) };
    }
    
    TASK_ID.with(|id| {
        if let Some(task_id) = id.get() {
            task_id
        } else {
            let new_id = TASK_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            id.set(Some(new_id));
            new_id
        }
    })
}

/// 异步读写锁
///
/// 公平且支持升级/降级的异步读写锁，带写锁可重入。
///
/// # 示例
///
/// ```rust
/// use rstream::locks::AsyncRWLock;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let lock = AsyncRWLock::new(true);
///
/// // 获取读锁
/// lock.acquire_read(None).await?;
/// // ... 读操作 ...
/// lock.release_read().await?;
///
/// // 获取写锁
/// lock.acquire_write(None).await?;
/// // ... 写操作 ...
/// lock.release_write().await?;
/// # Ok(())
/// # }
/// ```
pub struct AsyncRWLock {
    inner: Arc<Mutex<LockState>>,
    notify: Arc<Notify>,
}

struct LockState {
    /// 当前读者数量
    readers: u32,
    /// 当前写锁持有者任务ID（None表示无写锁）
    writer_task: Option<TaskId>,
    /// 写锁重入计数
    writer_recursion: u32,
    /// 等待写锁的任务数量
    write_waiters: u32,
    /// 每个任务持有的读锁数量
    task_read_counts: HashMap<TaskId, u32>,
    /// 是否启用公平模式
    fair: bool,
}

impl AsyncRWLock {
    /// 创建新的异步读写锁
    ///
    /// # 参数
    ///
    /// * `fair` - 是否启用公平模式。公平模式下，有写等待者时会阻止新读者进入
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// let lock = AsyncRWLock::new(true);
    /// ```
    pub fn new(fair: bool) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LockState {
                readers: 0,
                writer_task: None,
                writer_recursion: 0,
                write_waiters: 0,
                task_read_counts: HashMap::new(),
                fair,
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    /// 获取读锁
    ///
    /// 如果当前没有写锁持有者且（非公平模式或没有写等待者），则立即获取。
    /// 否则等待直到条件满足或超时。
    ///
    /// # 参数
    ///
    /// * `timeout_duration` - 超时时长（None表示无限等待）
    ///
    /// # 错误
    ///
    /// 如果超时则返回 `LockError::Timeout`
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let lock = AsyncRWLock::new(true);
    ///
    /// // 无限等待
    /// lock.acquire_read(None).await?;
    ///
    /// // 带超时
    /// lock.acquire_read(Some(Duration::from_secs(5))).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire_read(&self, timeout_duration: Option<Duration>) -> Result<(), LockError> {
        let acquire_fut = async {
            loop {
                {
                    let mut state = self.inner.lock().await;
                    let current = current_task_id();
                    
                    if state.writer_task.is_none() && (!state.fair || state.write_waiters == 0) {
                        state.readers += 1;
                        *state.task_read_counts.entry(current).or_insert(0) += 1;
                        return Ok::<(), LockError>(());
                    }
                }
                
                // 等待通知
                self.notify.notified().await;
            }
        };
        
        match timeout_duration {
            Some(dur) => timeout(dur, acquire_fut).await.map_err(|_| LockError::Timeout)?,
            None => acquire_fut.await,
        }
    }

    /// 尝试非阻塞地获取读锁
    ///
    /// 如果可以立即获取则返回 true，否则返回 false
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = AsyncRWLock::new(true);
    ///
    /// if lock.try_acquire_read().await {
    ///     println!("成功获取读锁");
    ///     lock.release_read().await.unwrap();
    /// }
    /// # }
    /// ```
    pub async fn try_acquire_read(&self) -> bool {
        let mut state = self.inner.lock().await;
        let current = current_task_id();
        
        if state.writer_task.is_none() && (!state.fair || state.write_waiters == 0) {
            state.readers += 1;
            *state.task_read_counts.entry(current).or_insert(0) += 1;
            true
        } else {
            false
        }
    }

    /// 释放读锁
    ///
    /// # 错误
    ///
    /// 如果当前任务未持有读锁则返回错误
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let lock = AsyncRWLock::new(true);
    /// lock.acquire_read(None).await?;
    /// lock.release_read().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn release_read(&self) -> Result<(), LockError> {
        let mut state = self.inner.lock().await;
        let current = current_task_id();
        
        let count = state.task_read_counts.get_mut(&current)
            .ok_or(LockError::ReleaseReadWithoutHolding)?;
        
        if *count == 0 {
            return Err(LockError::ReleaseReadWithoutHolding);
        }
        
        *count -= 1;
        state.readers -= 1;
        
        // 如果读者数归零，通知等待者
        if state.readers == 0 {
            drop(state);
            self.notify.notify_waiters();
        }
        
        Ok(())
    }

    /// 获取写锁（支持可重入）
    ///
    /// 如果当前任务已持有写锁，则增加重入计数。
    /// 否则等待直到没有读者和写者，然后获取写锁。
    ///
    /// # 参数
    ///
    /// * `timeout_duration` - 超时时长（None表示无限等待）
    ///
    /// # 错误
    ///
    /// 如果超时则返回 `LockError::Timeout`
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let lock = AsyncRWLock::new(true);
    /// lock.acquire_write(None).await?;
    /// // 可重入
    /// lock.acquire_write(None).await?;
    /// lock.release_write().await?;
    /// lock.release_write().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire_write(&self, timeout_duration: Option<Duration>) -> Result<(), LockError> {
        let current = current_task_id();
        
        let acquire_fut = async {
            // 先增加等待计数
            {
                let mut state = self.inner.lock().await;
                
                // 可重入检查
                if state.writer_task == Some(current) {
                    state.writer_recursion += 1;
                    return Ok::<(), LockError>(());
                }
                
                state.write_waiters += 1;
            }
            
            loop {
                {
                    let mut state = self.inner.lock().await;
                    
                    if state.writer_task.is_none() && state.readers == 0 {
                        state.writer_task = Some(current);
                        state.writer_recursion = 1;
                        state.write_waiters -= 1;
                        return Ok(());
                    }
                }
                
                // 等待通知
                self.notify.notified().await;
            }
        };
        
        match timeout_duration {
            Some(dur) => {
                match timeout(dur, acquire_fut).await {
                    Ok(result) => result,
                    Err(_) => {
                        // 超时时减少等待计数
                        let mut state = self.inner.lock().await;
                        state.write_waiters = state.write_waiters.saturating_sub(1);
                        Err(LockError::Timeout)
                    }
                }
            }
            None => acquire_fut.await,
        }
    }

    /// 尝试非阻塞地获取写锁
    ///
    /// 如果可以立即获取则返回 true，否则返回 false
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let lock = AsyncRWLock::new(true);
    ///
    /// if lock.try_acquire_write().await {
    ///     println!("成功获取写锁");
    ///     lock.release_write().await.unwrap();
    /// }
    /// # }
    /// ```
    pub async fn try_acquire_write(&self) -> bool {
        let mut state = self.inner.lock().await;
        let current = current_task_id();
        
        // 可重入检查
        if state.writer_task == Some(current) {
            state.writer_recursion += 1;
            return true;
        }
        
        if state.writer_task.is_none() && state.readers == 0 {
            state.writer_task = Some(current);
            state.writer_recursion = 1;
            true
        } else {
            false
        }
    }

    /// 释放写锁
    ///
    /// 减少重入计数，当计数归零时释放写锁。
    ///
    /// # 错误
    ///
    /// 如果当前任务不是写锁持有者则返回错误
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let lock = AsyncRWLock::new(true);
    /// lock.acquire_write(None).await?;
    /// lock.release_write().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn release_write(&self) -> Result<(), LockError> {
        let mut state = self.inner.lock().await;
        let current = current_task_id();
        
        if state.writer_task != Some(current) {
            return Err(LockError::ReleaseWriteByNonWriter);
        }
        
        state.writer_recursion -= 1;
        
        if state.writer_recursion == 0 {
            state.writer_task = None;
            drop(state);
            self.notify.notify_waiters();
        }
        
        Ok(())
    }

    /// 将读锁升级为写锁
    ///
    /// 当前任务必须持有读锁。升级时会释放一个读锁，然后等待获取写锁。
    /// 如果超时，会回滚读锁。
    ///
    /// # 参数
    ///
    /// * `timeout_duration` - 超时时长（None表示无限等待）
    ///
    /// # 错误
    ///
    /// - 如果当前任务未持有读锁则返回 `LockError::UpgradeWithoutReadLock`
    /// - 如果超时则返回 `LockError::Timeout`
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let lock = AsyncRWLock::new(true);
    /// lock.acquire_read(None).await?;
    /// // 升级为写锁
    /// lock.upgrade(None).await?;
    /// lock.release_write().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn upgrade(&self, timeout_duration: Option<Duration>) -> Result<(), LockError> {
        let current = current_task_id();
        
        // 检查是否持有读锁
        {
            let state = self.inner.lock().await;
            let count = state.task_read_counts.get(&current).copied().unwrap_or(0);
            if count == 0 {
                return Err(LockError::UpgradeWithoutReadLock);
            }
        }
        
        let upgrade_fut = async {
            // 先释放一个读锁
            {
                let mut state = self.inner.lock().await;
                
                // 如果已是写者，只需减少读计数
                if state.writer_task == Some(current) {
                    if let Some(count) = state.task_read_counts.get_mut(&current) {
                        *count = count.saturating_sub(1);
                    }
                    state.readers = state.readers.saturating_sub(1);
                    return Ok::<(), LockError>(());
                }
                
                // 减少读计数
                if let Some(count) = state.task_read_counts.get_mut(&current) {
                    *count = count.saturating_sub(1);
                }
                state.readers = state.readers.saturating_sub(1);
                state.write_waiters += 1;
            }
            
            // 等待成为写者
            loop {
                {
                    let mut state = self.inner.lock().await;
                    
                    if state.writer_task.is_none() && state.readers == 0 {
                        state.writer_task = Some(current);
                        state.writer_recursion = 1;
                        state.write_waiters -= 1;
                        return Ok(());
                    }
                }
                
                // 等待通知
                self.notify.notified().await;
            }
        };
        
        match timeout_duration {
            Some(dur) => {
                match timeout(dur, upgrade_fut).await {
                    Ok(result) => result,
                    Err(_) => {
                        // 超时时回滚读锁
                        let mut state = self.inner.lock().await;
                        state.readers += 1;
                        *state.task_read_counts.entry(current).or_insert(0) += 1;
                        state.write_waiters = state.write_waiters.saturating_sub(1);
                        Err(LockError::Timeout)
                    }
                }
            }
            None => upgrade_fut.await,
        }
    }

    /// 尝试非阻塞地升级读锁为写锁
    ///
    /// 如果可以立即升级则返回 true，否则返回 false
    pub async fn try_upgrade(&self) -> Result<bool, LockError> {
        let mut state = self.inner.lock().await;
        let current = current_task_id();
        
        let count = state.task_read_counts.get(&current).copied().unwrap_or(0);
        if count == 0 {
            return Err(LockError::UpgradeWithoutReadLock);
        }
        
        // 如果已是写者
        if state.writer_task == Some(current) {
            if let Some(c) = state.task_read_counts.get_mut(&current) {
                *c = c.saturating_sub(1);
            }
            state.readers = state.readers.saturating_sub(1);
            return Ok(true);
        }
        
        // 尝试升级
        if state.writer_task.is_none() && state.readers == 1 {
            if let Some(c) = state.task_read_counts.get_mut(&current) {
                *c = c.saturating_sub(1);
            }
            state.readers = 0;
            state.writer_task = Some(current);
            state.writer_recursion = 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 将写锁降级为读锁
    ///
    /// 当前任务必须持有写锁且重入计数为1。降级后会持有一个读锁。
    ///
    /// # 错误
    ///
    /// - 如果当前任务不是写锁持有者则返回 `LockError::DowngradeByNonWriter`
    /// - 如果写锁重入计数大于1则返回 `LockError::DowngradeWithNestedWriteLocks`
    ///
    /// # 示例
    ///
    /// ```rust
    /// use rstream::locks::AsyncRWLock;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let lock = AsyncRWLock::new(true);
    /// lock.acquire_write(None).await?;
    /// // 降级为读锁
    /// lock.downgrade().await?;
    /// lock.release_read().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn downgrade(&self) -> Result<(), LockError> {
        let mut state = self.inner.lock().await;
        let current = current_task_id();
        
        if state.writer_task != Some(current) {
            return Err(LockError::DowngradeByNonWriter);
        }
        
        if state.writer_recursion > 1 {
            return Err(LockError::DowngradeWithNestedWriteLocks);
        }
        
        // 降级为读锁
        state.writer_task = None;
        state.writer_recursion = 0;
        state.readers += 1;
        *state.task_read_counts.entry(current).or_insert(0) += 1;
        
        drop(state);
        self.notify.notify_waiters();
        
        Ok(())
    }
}

impl Clone for AsyncRWLock {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            notify: Arc::clone(&self.notify),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_lock() {
        let lock = AsyncRWLock::new(true);
        
        lock.acquire_read(None).await.unwrap();
        lock.acquire_read(None).await.unwrap();
        
        lock.release_read().await.unwrap();
        lock.release_read().await.unwrap();
    }

    #[tokio::test]
    async fn test_write_lock() {
        let lock = AsyncRWLock::new(true);
        
        lock.acquire_write(None).await.unwrap();
        lock.release_write().await.unwrap();
    }

    #[tokio::test]
    async fn test_write_reentrant() {
        let lock = AsyncRWLock::new(true);
        
        lock.acquire_write(None).await.unwrap();
        lock.acquire_write(None).await.unwrap();
        lock.release_write().await.unwrap();
        lock.release_write().await.unwrap();
    }

    #[tokio::test]
    async fn test_upgrade() {
        let lock = AsyncRWLock::new(true);
        
        lock.acquire_read(None).await.unwrap();
        lock.upgrade(None).await.unwrap();
        lock.release_write().await.unwrap();
    }

    #[tokio::test]
    async fn test_downgrade() {
        let lock = AsyncRWLock::new(true);
        
        lock.acquire_write(None).await.unwrap();
        lock.downgrade().await.unwrap();
        lock.release_read().await.unwrap();
    }
}
