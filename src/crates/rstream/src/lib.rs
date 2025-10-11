//! # rstream - Rust 异步流管理器
//!
//! 这是一个高性能的异步上下游流管理器，基于 Python rstream 的设计理念，
//! 使用 Rust 实现以达到极致的性能和安全性。
//!
//! ## 特性
//!
//! - **零隐式转换**: 所有转换都是显式的，确保最高性能
//! - **公平读写锁**: 支持升级/降级，带写锁可重入
//! - **依赖管理**: 自动管理上下游资源依赖关系
//! - **性能指标**: 详细的锁等待和持有时间统计
//! - **过程宏支持**: 提供装饰器风格的宏简化使用
//! - **完整错误处理**: 使用 error 模块进行统一错误处理
//! - **测试驱动**: 所有代码都有完整的测试覆盖
//!
//! ## 核心概念
//!
//! ### AsyncRWLock
//!
//! 公平且支持升级/降级的异步读写锁：
//! - 支持读锁的并发获取
//! - 支持写锁的可重入
//! - 支持读锁升级为写锁
//! - 支持写锁降级为读锁
//! - 所有操作都支持超时
//!
//! ### StreamRegistry
//!
//! 资源注册表，管理所有资源条目：
//! - 单例模式，全局唯一
//! - 管理资源的锁和信号量
//! - 维护上下游依赖关系
//! - 跟踪全局引用计数
//!
//! ### StreamHandle
//!
//! 流句柄，用于操作锁：
//! - 跟踪读/写锁的重入深度
//! - 自动记录性能指标
//! - 提供安全释放机制
//! - 支持升级作用域
//!
//! ## 使用示例
//!
//! ### 基本使用
//!
//! ```rust
//! use rstream::registry::{StreamRegistry, StreamHandle};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建注册表
//! let registry = StreamRegistry::new();
//!
//! // 获取或创建资源
//! let entry = registry.get_or_create(123, None).await;
//!
//! // 创建流句柄
//! let mut handle = StreamHandle::new(entry);
//!
//! // 获取读锁
//! handle.acquire_read(None).await?;
//!
//! // ... 执行读操作 ...
//!
//! // 升级为写锁
//! handle.upgrade(None).await?;
//!
//! // ... 执行写操作 ...
//!
//! // 降级为读锁
//! handle.downgrade().await?;
//!
//! // 释放锁
//! handle.safe_release().await;
//! # Ok(())
//! # }
//! ```
//!
//! ### 使用读写锁
//!
//! ```rust
//! use rstream::locks::AsyncRWLock;
//! use std::time::Duration;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let lock = AsyncRWLock::new(true);
//!
//! // 获取读锁（带超时）
//! lock.acquire_read(Some(Duration::from_secs(5))).await?;
//! lock.release_read().await?;
//!
//! // 获取写锁
//! lock.acquire_write(None).await?;
//! lock.release_write().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 设计原则
//!
//! 1. **显式优于隐式**: 所有转换和操作都是显式的
//! 2. **性能第一**: 避免不必要的分配和复制
//! 3. **类型安全**: 利用 Rust 的类型系统确保正确性
//! 4. **错误处理**: 使用 Result 类型明确错误处理
//! 5. **文档完备**: 所有公开 API 都有详细的中文文档
//! 6. **测试驱动**: 所有功能都有对应的测试

pub mod locks;
pub mod registry;

// 重新导出常用类型
pub use locks::{AsyncRWLock, LockError};
pub use registry::{StreamRegistry, ResourceEntry, StreamHandle, Metrics, LockMode, TimeoutStrategy};

// 重新导出过程宏
pub use rstream_derive::{lower_stream, upper_stream};
