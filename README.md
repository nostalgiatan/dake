# dake - 数据处理和打包工具

一个基于 DSL 的高性能数据处理和打包工具，使用 Rust 实现。

## 特性

- ✅ **完整的 DSL 支持**: 类似 CMake 的 DSL 语法，专为数据处理设计
- ✅ **变量管理**: 支持局部变量和环境变量，具有明确的优先级
- ✅ **控制流**: IF/ELIF/ELSE 条件语句
- ✅ **正则表达式缓存**: 高性能的正则表达式编译和缓存，避免重复编译
- ✅ **文件加密**: 使用 XChaCha20-Poly1305 加密算法
- ✅ **安全路径处理**: 禁止使用 ../, .../ 和 ./
- ✅ **零隐式转换**: 所有操作都是显式的，确保最高性能
- ✅ **内存安全**: 避免所有危险的 unwrap()，完整的错误处理
- ✅ **测试驱动**: 45 个测试用例覆盖核心功能
- ✅ **中文文档**: 所有 API 都有详细的中文文档
- ✅ **极致性能**: 使用细粒度锁和零成本抽象

## 安装

### 前置要求

- Rust 1.70.0 或更高版本
- Cargo

### 从源码构建

```bash
git clone https://github.com/nostalgiatan/dake.git
cd dake
cargo build --release
```

构建完成后，可执行文件位于 `target/release/dake`。

## 使用方法

### 命令行界面

```bash
# 检查 DSL 文件语法
dake check <FILE>

# 显示 DSL 文件的 AST
dake ast <FILE>

# 运行 DSL 文件
dake run <FILE>

# 运行时显示详细信息
dake run <FILE> --verbose
```

### DSL 语法

#### 变量定义

```dsl
# 局部变量
set(PROJECT_NAME, "my-project")
set(VERSION, "1.0.0")

# 环境变量
set.env(OUTPUT_DIR, "/tmp/output")
```

#### 控制流

```dsl
IF ${CONDITION}:
    PRINT("条件为真")
ELIF ${OTHER_CONDITION}:
    PRINT("另一个条件为真")
ELSE:
    PRINT("所有条件都不满足")
```

#### 数据包定义

```dsl
lib
    name:"example-lib"
    version:"1.0.0"
    desc:"数据包描述"
    repo:"https://github.com/user/repo"
    keywords:["data", "package"]
    readme:"README.md"
    out_dir:"./output"
```

#### 仓库定义

```dsl
repo
    name:"my-repo"
    capacity:1024 MiB
    max_pkgs:100
```

#### 文件操作

```dsl
# 文件列表
FILES(
    "data/file1.txt",
    "data/file2.txt",
    "${VAR}/file3.txt"
)

# 递归获取所有文件，排除指定路径
FILES.ALL(other:["exclude_dir", "temp"])

# 启用文件加密
FILES.ENCRY()
```

#### 日志操作

```dsl
# 初始化日志
LOG.INIT("./logs", true)

# 记录信息
LOG.INFO("处理开始")

# 记录错误
LOG.ERROR("发生错误")
```

#### 数据操作

```dsl
# 定义数据操作
DATA.DO(clean_data, "data.csv", clean)

# 定义数据管道
DATA.PIPE.process(do.clean_data, do.transform, do.validate)

# 正则表达式定义（自动缓存）
DATA.RE("\\d+")
DATA.RE("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$")

# 执行数据管道
DOING(DATA.PIPE.process)
```

#### 命令操作

```dsl
# 定义命令动作
COMM.ACTION(
    PRINT("开始处理")
    DOING(DATA.PIPE.process)
    LOG.INFO("处理完成")
)

# 执行系统命令
COMM.CMD("ls", "-la", "/tmp")
```

#### 并发执行

```dsl
# 并发执行多个操作
AWAIT(
    DATA.PIPE.pipeline1,
    DATA.PIPE.pipeline2,
    DATA.PIPE.pipeline3
)
```

#### 错误处理

```dsl
# 定义错误
ERROR DATAERROR(print:"数据处理错误", e:error)

# 捕获错误
CATCH ERROR.DATAERROR:
    LOG.ERROR(e)
    PRINT("错误已处理")
```

#### 打印输出

```dsl
# 基本打印
PRINT("Hello, World!")

# 变量插值
set(NAME, "Alice")
PRINT("Hello, ${NAME}!")

# Emoji 支持
PRINT("你好👋")
```

#### 注释

```dsl
# 这是单行注释
set(VAR, "value")  # 行末注释也支持
```

## 示例

查看 `examples/` 目录获取完整示例：

- `example.dsl`: 综合示例，展示各种 DSL 特性
- `simple_lib.dsl`: 简单的数据包定义示例
- `basic_regex.dsl`: 基础正则表达式使用示例
- `regex_validation.dsl`: 正则表达式验证示例（邮箱、URL、电话号码等）

运行示例：

```bash
cargo run -- check examples/example.dsl
cargo run -- run examples/example.dsl
```

## 项目结构

```
dake/
├── src/
│   ├── main.rs              # 主程序入口
│   ├── cli.rs               # 命令行接口
│   ├── dsl/                 # DSL 解析器
│   │   ├── mod.rs
│   │   ├── ast.rs           # 抽象语法树定义
│   │   ├── lexer.rs         # 词法分析器
│   │   └── parser.rs        # 语法分析器
│   ├── executor/            # 执行器
│   │   ├── mod.rs
│   │   ├── context.rs       # 执行上下文
│   │   ├── crypto.rs        # 加密操作
│   │   ├── file_ops.rs      # 文件操作
│   │   └── executor.rs      # 执行器核心
│   └── data/                # 数据处理模块
│       ├── mod.rs
│       ├── regex_cache.rs   # 正则表达式缓存
│       └── README.md        # 数据模块文档
├── src/crates/              # 工具库
│   ├── error/               # 错误处理框架
│   ├── rstream/             # 异步流管理
│   ├── transaction/         # 事务管理
│   └── sys/                 # 系统信息监控
├── examples/                # 示例文件
│   ├── example.dsl
│   ├── simple_lib.dsl
│   ├── basic_regex.dsl
│   └── regex_validation.dsl
├── Cargo.toml
└── README.md
```

## 开发

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_parse_lib

# 显示测试输出
cargo test -- --nocapture
```

### 代码规范

本项目遵循以下规范：

1. **测试驱动开发**: 所有功能都有对应的测试
2. **实用主义**: 编译不产生警告（允许合理的 dead_code 警告）
3. **完整文档**: 所有公共 API 都有中文文档
4. **禁止简化实现**: 所有功能都是完整实现
5. **内存安全**: 禁止使用 unwrap()，使用 Result 处理错误
6. **显式优于隐式**: 避免隐式类型转换

### 生成文档

```bash
cargo doc --open
```

## 技术栈

- **语言**: Rust 2021 Edition
- **命令行**: clap 4.5
- **加密**: chacha20poly1305 0.10
- **序列化**: serde 1.0
- **正则表达式**: regex 1.12 (标准库)
- **解析**: 自定义词法分析器和语法分析器
- **版本**: semver 1.0

## 设计原则

### 1. 显式优于隐式

所有类型转换和操作都是显式的，避免隐式行为带来的性能开销和不确定性。

### 2. 内存安全第一

- 禁止使用 unwrap()
- 使用 Result 类型进行错误处理
- 严格的生命周期管理
- 避免不必要的克隆和分配

### 3. 性能优化

- 零成本抽象
- 避免不必要的分配
- 使用栈上分配优于堆分配
- 静态分发优于动态分发

### 4. 安全性

- 路径验证：禁止 ../、.../ 和 ./
- 环境变量访问控制：禁止隐式读取进程环境
- 加密支持：XChaCha20-Poly1305 算法
- Deterministic 模式支持密钥管理

## 变量优先级

变量解析遵循以下优先级（从高到低）：

1. **局部变量**: 通过 `set()` 定义
2. **环境变量**: 通过 `set.env()` 定义
3. **父级上下文**: 作用域链中的上级变量
4. **进程环境**: 仅当显式声明过时才可访问

## 限制和注意事项

1. **路径安全**: 文件路径禁止使用 `../`、`.../` 和 `./`
2. **环境变量**: 禁止隐式读取进程环境变量
3. **命名空间**: 同一命名空间内的名称必须唯一
4. **子包限定**: 子包中的操作必须使用包名前缀（如 `acme.sales.clean_customers`）

## 路线图

### 已完成 ✅

- [x] DSL 词法分析器
- [x] DSL 语法分析器
- [x] AST 定义
- [x] 执行上下文和变量管理
- [x] 控制流执行
- [x] 文件加密（XChaCha20-Poly1305）
- [x] 文件操作
- [x] 正则表达式缓存（DATA.RE）
- [x] 命令行接口
- [x] 测试套件（45个测试）
- [x] 零编译警告

### 进行中 🚧

- [ ] 完整的数据管道执行
- [ ] COMM.CMD 完整实现
- [ ] AWAIT 并发执行（Tokio）
- [ ] 数据包打包功能

### 计划中 📋

- [ ] 包仓库管理
- [ ] 网络传输支持
- [ ] 增量更新
- [ ] 压缩支持
- [ ] 签名验证
- [ ] Web 界面

## 贡献

欢迎提交 Issue 和 Pull Request！

在提交 PR 之前，请确保：

1. 所有测试通过：`cargo test`
2. 代码格式正确：`cargo fmt`
3. 没有编译警告（除合理的 dead_code 警告）
4. 添加了适当的测试
5. 更新了文档

## 许可证

本项目采用 MIT 许可证。详见 LICENSE 文件。

## 致谢

- 感谢 Rust 社区提供的优秀工具和库
- 灵感来源于 CMake 的 DSL 设计
- 使用了项目内部的 error、rstream、transaction 和 sys 工具库

## 联系方式

- GitHub Issues: https://github.com/nostalgiatan/dake/issues
- 作者: nostalgiatan

## 更新日志

### 0.1.0 (2024)

- ✅ 初始版本
- ✅ 完整的 DSL 解析器
- ✅ 基础执行器功能
- ✅ 文件加密支持
- ✅ 正则表达式缓存（DATA.RE）
- ✅ 命令行工具
- ✅ 45 个测试用例
- ✅ 零编译警告
- ✅ 完整的中文文档
