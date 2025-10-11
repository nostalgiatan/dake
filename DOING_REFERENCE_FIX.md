# DOING 引用解析修复

## 问题描述

在修复前，Parser 在解析 `DOING(DATA.PIPE.process)` 时会把引用解析成完整的 "DATA.PIPE.process"，而 Executor 中的 `DataPipe` 存储的键是简单名称 "process"。当 `execute_doing` 直接用完整引用当 key 查找 `self.data_pipes.get(reference)` 时，会无法命中，导致错误：

```
未找到操作或命令: DATA.PIPE.process
```

## 解决方案

### 1. 添加引用规范化函数

在 `src/executor/executor.rs` 中添加了 `normalize_reference` 函数，该函数能够：

- 识别并剥离以下前缀：
  - `DATA.PIPE.`
  - `DATA.DO.`
  - `COMM.ACTION.`
  - `COMM.`

- 返回规范化后的简单名称

```rust
/// 规范化引用名称
/// 
/// 将 "DATA.PIPE.name"、"DATA.DO.name"、"COMM.ACTION.name" 等完整引用
/// 转换为简单名称 "name"，以便在 HashMap 中查找
fn normalize_reference(reference: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "DATA.PIPE.",
        "DATA.DO.",
        "COMM.ACTION.",
        "COMM.",
    ];
    
    for prefix in PREFIXES {
        if let Some(stripped) = reference.strip_prefix(prefix) {
            return stripped;
        }
    }
    
    reference
}
```

### 2. 修改 execute_doing 函数

更新 `execute_doing` 函数，在查找之前先规范化引用：

```rust
fn execute_doing(&mut self, reference: &str) -> Result<(), ExecutionError> {
    // 规范化引用名称
    let normalized = Self::normalize_reference(reference);
    
    if let Some(pipe) = self.data_pipes.get(normalized).cloned() {
        // ...
    }
    
    if let Some(cmd) = self.commands.get(normalized).cloned() {
        // ...
    }
    
    Err(ExecutionError::new(4005, format!("未找到操作或命令: {}", reference)))
}
```

## 测试覆盖

添加了以下测试确保修复有效：

### 1. `test_doing_with_data_pipe_reference`
测试使用完整引用 `DOING(DATA.PIPE.process)` 调用管道

### 2. `test_doing_with_comm_action_reference`
测试使用完整引用 `DOING(COMM.ACTION.test_action)` 调用命令动作

### 3. `test_doing_with_simple_reference`
测试使用简单引用 `DOING(simple)` 仍然能正常工作

### 4. `test_normalize_reference`
单元测试，验证 `normalize_reference` 函数对各种输入的处理：
- 带前缀的引用
- 不带前缀的引用
- 包含点但不匹配前缀的引用
- 空字符串

### 5. `test_await_with_normalized_references`
测试 `AWAIT` 语句能正确处理多个带前缀的引用

### 6. `test_integration_parser_executor_with_references`
端到端集成测试，验证从 DSL 解析到执行的完整流程

## 影响范围

### 受益功能
- `DOING` 语句：现在支持完整引用和简单引用
- `AWAIT` 语句：自动继承修复（因为它调用 `execute_doing`）

### 向后兼容性
✅ 完全向后兼容。使用简单引用（如 `DOING(process)`）的现有代码继续正常工作。

### 新支持的语法
```dsl
# 现在这些都可以工作：
DOING(DATA.PIPE.process)
DOING(DATA.DO.operation)
DOING(COMM.ACTION.myaction)
DOING(COMM.command)

# 以前只支持：
DOING(process)
DOING(operation)
DOING(myaction)
DOING(command)
```

## 测试结果

所有 130 个测试通过：
- 124 个原有测试保持通过（无回归）
- 6 个新增测试全部通过

```
test result: ok. 130 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 性能影响

性能影响极小：
- `normalize_reference` 使用 `strip_prefix`，是 O(1) 操作
- 只在引用查找时调用一次
- 无额外内存分配（返回原字符串的切片）

## 代码规范

✅ 所有更改遵循项目规范：
- 使用 Rust 编程语言
- 遵循测试驱动原则（先编写失败的测试，再实现修复）
- 无编译警告
- 完整的中文 API 文档和注释
- 禁止使用 `unwrap()`，确保内存安全

## 相关问题

此修复解决了问题清单中的第一项：
> DOING 引用解析存在功能性缺陷：Parser 在 DOING(DATA.PIPE.process) 中会把 reference 解析成 "DATA.PIPE.process"，而 Executor::DataPipe 存的是 name="process"。

## 示例

```dsl
# 定义数据管道
DATA.PIPE.process1()
DATA.PIPE.process2()

# 定义命令动作
COMM.ACTION.myaction(
    PRINT("My action executed")
)

# 使用完整引用调用（现在可以工作了）
DOING(DATA.PIPE.process1)
DOING(DATA.PIPE.process2)
DOING(COMM.ACTION.myaction)

# 使用简单引用调用（也可以工作）
DOING(process1)
DOING(myaction)

# 并发执行（AWAIT 也支持完整引用）
AWAIT(DATA.PIPE.process1, DATA.PIPE.process2, COMM.ACTION.myaction)
```
