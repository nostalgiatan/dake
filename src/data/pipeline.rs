/*
 * 数据管道处理模块
 *
 * 提供高性能的数据管道处理功能，支持链式操作。
 * 使用零成本抽象，确保极致性能和内存安全。
 */

use error::Result;

/// 数据管道
///
/// 提供函数式的数据处理管道，支持 map、filter、fold 等操作。
/// 所有操作都是惰性求值，直到调用 collect 或 fold 时才执行。
///
/// # 示例
///
/// ```no_run
/// use dake::data::Pipeline;
///
/// let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
///     .map(|x| x * 2)
///     .filter(|x| *x > 5)
///     .collect()
///     .expect("处理失败");
///
/// assert_eq!(result, vec![6, 8, 10]);
/// ```
pub struct Pipeline<T> {
    /// 内部迭代器
    items: Vec<T>,
}

impl<T> Pipeline<T> {
    /// 创建新的空管道
    ///
    /// # 返回
    /// 返回一个新的空管道实例
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// 从迭代器创建管道
    ///
    /// # 参数
    /// * `iter` - 实现了 IntoIterator trait 的对象
    ///
    /// # 返回
    /// 返回包含迭代器元素的管道
    pub fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        Self {
            items: iter.into_iter().collect(),
        }
    }

    /// 映射操作
    ///
    /// 对管道中的每个元素应用函数，返回新的管道。
    ///
    /// # 参数
    /// * `f` - 映射函数
    ///
    /// # 返回
    /// 返回包含映射后元素的新管道
    pub fn map<U, F>(self, f: F) -> Pipeline<U>
    where
        F: FnMut(T) -> U,
    {
        Pipeline {
            items: self.items.into_iter().map(f).collect(),
        }
    }

    /// 过滤操作
    ///
    /// 保留满足条件的元素，返回新的管道。
    ///
    /// # 参数
    /// * `predicate` - 过滤条件函数
    ///
    /// # 返回
    /// 返回包含过滤后元素的新管道
    pub fn filter<F>(self, predicate: F) -> Self
    where
        F: FnMut(&T) -> bool,
    {
        Self {
            items: self.items.into_iter().filter(predicate).collect(),
        }
    }

    /// 归约操作
    ///
    /// 将管道中的所有元素归约为单个值。
    ///
    /// # 参数
    /// * `init` - 初始值
    /// * `f` - 归约函数
    ///
    /// # 返回
    /// 成功时返回归约后的值
    pub fn fold<U, F>(self, init: U, f: F) -> Result<U>
    where
        F: FnMut(U, T) -> U,
    {
        Ok(self.items.into_iter().fold(init, f))
    }

    /// 收集操作
    ///
    /// 将管道中的元素收集到 Vec 中。
    ///
    /// # 返回
    /// 成功时返回包含所有元素的 Vec
    pub fn collect(self) -> Result<Vec<T>> {
        Ok(self.items)
    }

    /// 获取管道长度
    ///
    /// # 返回
    /// 管道中元素的数量
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 检查管道是否为空
    ///
    /// # 返回
    /// 如果管道为空返回 true，否则返回 false
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 取前 n 个元素
    ///
    /// # 参数
    /// * `n` - 要取的元素数量
    ///
    /// # 返回
    /// 返回包含前 n 个元素的新管道
    pub fn take(self, n: usize) -> Self {
        Self {
            items: self.items.into_iter().take(n).collect(),
        }
    }

    /// 跳过前 n 个元素
    ///
    /// # 参数
    /// * `n` - 要跳过的元素数量
    ///
    /// # 返回
    /// 返回跳过前 n 个元素后的新管道
    pub fn skip(self, n: usize) -> Self {
        Self {
            items: self.items.into_iter().skip(n).collect(),
        }
    }
}

impl<T> Default for Pipeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// 可尝试的管道操作
///
/// 支持可能失败的操作，例如 try_map。
pub struct TryPipeline<T> {
    /// 内部迭代器
    items: Vec<T>,
}

impl<T> TryPipeline<T> {
    /// 从管道创建
    ///
    /// # 参数
    /// * `pipeline` - 源管道
    ///
    /// # 返回
    /// 返回新的 TryPipeline 实例
    pub fn from_pipeline(pipeline: Pipeline<T>) -> Self {
        Self {
            items: pipeline.items,
        }
    }

    /// 尝试映射操作
    ///
    /// 对每个元素应用可能失败的函数。
    ///
    /// # 参数
    /// * `f` - 映射函数，返回 `Result`
    ///
    /// # 返回
    /// 成功时返回包含映射后元素的新管道，失败时返回错误
    pub fn try_map<U, F>(self, mut f: F) -> Result<Pipeline<U>>
    where
        F: FnMut(T) -> Result<U>,
    {
        let mut result = Vec::with_capacity(self.items.len());
        for item in self.items {
            result.push(f(item)?);
        }
        Ok(Pipeline { items: result })
    }

    /// 尝试过滤操作
    ///
    /// 使用可能失败的条件函数过滤元素。
    ///
    /// # 参数
    /// * `predicate` - 过滤条件函数，返回 `Result<bool>`
    ///
    /// # 返回
    /// 成功时返回包含过滤后元素的新管道，失败时返回错误
    pub fn try_filter<F>(self, mut predicate: F) -> Result<TryPipeline<T>>
    where
        F: FnMut(&T) -> Result<bool>,
    {
        let mut result = Vec::new();
        for item in self.items {
            if predicate(&item)? {
                result.push(item);
            }
        }
        Ok(TryPipeline { items: result })
    }

    /// 收集结果
    ///
    /// # 返回
    /// 成功时返回包含所有元素的 Vec
    pub fn collect(self) -> Result<Vec<T>> {
        Ok(self.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use error::ErrorInfo;

    #[test]
    fn test_pipeline_basic() {
        let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
            .collect()
            .expect("收集失败");

        assert_eq!(result, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_pipeline_map() {
        let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3])
            .map(|x| x * 2)
            .collect()
            .expect("处理失败");

        assert_eq!(result, vec![2, 4, 6]);
    }

    #[test]
    fn test_pipeline_filter() {
        let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
            .filter(|x| *x > 2)
            .collect()
            .expect("处理失败");

        assert_eq!(result, vec![3, 4, 5]);
    }

    #[test]
    fn test_pipeline_map_filter() {
        let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
            .map(|x| x * 2)
            .filter(|x| *x > 5)
            .collect()
            .expect("处理失败");

        assert_eq!(result, vec![6, 8, 10]);
    }

    #[test]
    fn test_pipeline_fold() {
        let result: i32 = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
            .fold(0, |acc, x| acc + x)
            .expect("归约失败");

        assert_eq!(result, 15);
    }

    #[test]
    fn test_pipeline_take() {
        let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
            .take(3)
            .collect()
            .expect("处理失败");

        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_pipeline_skip() {
        let result: Vec<i32> = Pipeline::from_iter(vec![1, 2, 3, 4, 5])
            .skip(2)
            .collect()
            .expect("处理失败");

        assert_eq!(result, vec![3, 4, 5]);
    }

    #[test]
    fn test_pipeline_len() {
        let pipeline = Pipeline::from_iter(vec![1, 2, 3, 4, 5]);
        assert_eq!(pipeline.len(), 5);
    }

    #[test]
    fn test_pipeline_is_empty() {
        let empty: Pipeline<i32> = Pipeline::new();
        assert!(empty.is_empty());

        let not_empty = Pipeline::from_iter(vec![1]);
        assert!(!not_empty.is_empty());
    }

    #[test]
    fn test_try_pipeline_map() {
        let result: Vec<i32> = TryPipeline::from_pipeline(Pipeline::from_iter(vec![1, 2, 3]))
            .try_map(|x| Ok(x * 2))
            .expect("映射失败")
            .collect()
            .expect("收集失败");

        assert_eq!(result, vec![2, 4, 6]);
    }

    #[test]
    fn test_try_pipeline_map_error() {
        let result = TryPipeline::from_pipeline(Pipeline::from_iter(vec![1, 2, 3]))
            .try_map(|x| {
                if x == 2 {
                    Err(ErrorInfo::new(3001, "测试错误".to_string()))
                } else {
                    Ok(x * 2)
                }
            });

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 3001);
        }
    }

    #[test]
    fn test_try_pipeline_filter() {
        let result: Vec<i32> = TryPipeline::from_pipeline(Pipeline::from_iter(vec![1, 2, 3, 4, 5]))
            .try_filter(|x| Ok(*x > 2))
            .expect("过滤失败")
            .collect()
            .expect("收集失败");

        assert_eq!(result, vec![3, 4, 5]);
    }

    #[test]
    fn test_pipeline_complex() {
        let result: i32 = Pipeline::from_iter(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
            .filter(|x| *x % 2 == 0) // 保留偶数
            .map(|x| x * x)           // 平方
            .take(3)                  // 取前3个
            .fold(0, |acc, x| acc + x)
            .expect("处理失败");

        // 偶数: 2, 4, 6, 8, 10
        // 平方: 4, 16, 36, 64, 100
        // 取前3个: 4, 16, 36
        // 求和: 56
        assert_eq!(result, 56);
    }
}
