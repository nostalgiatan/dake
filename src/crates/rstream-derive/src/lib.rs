//! # rstream 过程宏
//!
//! 为 rstream 提供标记宏支持。
//!
//! ## 设计理念
//!
//! 遵循 rstream 的核心原则："显式优于隐式"，这些宏不会自动注入锁管理代码。
//! 它们作为文档和标记工具，帮助开发者：
//!
//! 1. 标识函数的用途（上游/下游流处理）
//! 2. 添加一致的文档说明
//! 3. 提供代码可读性和维护性
//!
//! ## 性能保证
//!
//! - **零成本抽象**: 宏不添加任何运行时开销
//! - **显式锁管理**: 所有锁操作都是显式调用，确保开发者完全控制
//! - **编译时优化**: 宏展开后的代码可被编译器完全优化
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use rstream::{lower_stream, upper_stream, StreamHandle, LockError};
//!
//! #[lower_stream]
//! async fn read_data(handle: &mut StreamHandle) -> Result<Vec<u8>, LockError> {
//!     handle.acquire_read(None).await?;
//!     // 读取数据...
//!     handle.release_read()?;
//!     Ok(vec![])
//! }
//!
//! #[upper_stream]
//! async fn write_data(handle: &mut StreamHandle, data: Vec<u8>) -> Result<(), LockError> {
//!     handle.acquire_write(None).await?;
//!     // 写入数据...
//!     handle.release_write()?;
//!     Ok(())
//! }
//! ```

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, Attribute};

/// lower_stream 装饰器宏
/// 
/// 用于标记下游流函数。该宏为函数添加文档和类型标记，
/// 表明该函数用于下游流处理。下游流处理通常使用读锁进行数据读取。
/// 
/// # 说明
/// 
/// 这是一个标记宏，用于：
/// - 标识函数用途（下游流处理）
/// - 添加文档说明
/// - 为函数提供一致的调用约定
/// 
/// 在实际使用中，下游流函数应该：
/// - 使用 `StreamHandle::acquire_read()` 获取读锁
/// - 在函数结束时使用 `StreamHandle::release_read()` 释放锁
/// - 或使用 RAII 模式通过 Drop trait 自动释放锁
/// 
/// # 示例
/// 
/// ```rust,ignore
/// use rstream::{lower_stream, StreamHandle, LockError};
/// 
/// #[lower_stream]
/// async fn process_downstream(handle: &mut StreamHandle) -> Result<(), LockError> {
///     // 手动获取读锁
///     handle.acquire_read(None).await?;
///     
///     // 处理下游数据...
///     
///     // 手动释放读锁
///     handle.release_read()?;
///     Ok(())
/// }
/// ```
/// 
/// # 性能
/// 
/// 此宏不添加任何运行时开销，仅用于文档和标记目的。
/// 实际的锁管理由显式调用完成，确保零成本抽象。
#[proc_macro_attribute]
pub fn lower_stream(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    
    let fn_vis = &input.vis;
    let fn_sig = &input.sig;
    let fn_block = &input.block;
    let fn_attrs: Vec<&Attribute> = input.attrs.iter().collect();
    
    // 标记为下游流函数，不添加额外逻辑以保持零成本抽象
    // 锁管理通过显式调用完成，确保性能最优
    let expanded = quote! {
        /// 下游流处理函数
        /// 
        /// 此函数用于处理下游流数据。
        /// 建议使用读锁（`acquire_read`）进行数据访问。
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            #fn_block
        }
    };
    
    TokenStream::from(expanded)
}

/// upper_stream 装饰器宏
/// 
/// 用于标记上游流函数。该宏为函数添加文档和类型标记，
/// 表明该函数用于上游流处理。上游流处理通常需要写锁进行数据修改。
/// 
/// # 说明
/// 
/// 这是一个标记宏，用于：
/// - 标识函数用途（上游流处理）
/// - 添加文档说明
/// - 为函数提供一致的调用约定
/// 
/// 在实际使用中，上游流函数应该：
/// - 使用 `StreamHandle::acquire_write()` 获取写锁
/// - 或使用 `StreamHandle::upgrade()` 从读锁升级到写锁
/// - 在函数结束时使用 `StreamHandle::release_write()` 释放锁
/// - 或使用 `StreamHandle::downgrade()` 降级回读锁
/// 
/// # 示例
/// 
/// ```rust,ignore
/// use rstream::{upper_stream, StreamHandle, LockError};
/// 
/// #[upper_stream]
/// async fn process_upstream(handle: &mut StreamHandle) -> Result<(), LockError> {
///     // 手动获取写锁
///     handle.acquire_write(None).await?;
///     
///     // 处理上游数据并修改状态...
///     
///     // 手动释放写锁
///     handle.release_write()?;
///     Ok(())
/// }
/// ```
/// 
/// # 锁升级示例
/// 
/// ```rust,ignore
/// #[upper_stream]
/// async fn process_with_upgrade(handle: &mut StreamHandle) -> Result<(), LockError> {
///     // 先使用读锁读取
///     handle.acquire_read(None).await?;
///     
///     // 需要修改时升级到写锁
///     handle.upgrade(None).await?;
///     
///     // 修改数据...
///     
///     // 降级回读锁
///     handle.downgrade().await?;
///     
///     // 继续读取...
///     
///     handle.release_read()?;
///     Ok(())
/// }
/// ```
/// 
/// # 性能
/// 
/// 此宏不添加任何运行时开销，仅用于文档和标记目的。
/// 实际的锁管理由显式调用完成，确保零成本抽象。
#[proc_macro_attribute]
pub fn upper_stream(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    
    let fn_vis = &input.vis;
    let fn_sig = &input.sig;
    let fn_block = &input.block;
    let fn_attrs: Vec<&Attribute> = input.attrs.iter().collect();
    
    // 标记为上游流函数，不添加额外逻辑以保持零成本抽象
    // 锁管理通过显式调用完成，确保性能最优
    let expanded = quote! {
        /// 上游流处理函数
        /// 
        /// 此函数用于处理上游流数据。
        /// 建议使用写锁（`acquire_write`）或锁升级（`upgrade`）进行数据修改。
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            #fn_block
        }
    };
    
    TokenStream::from(expanded)
}

