# 示例 DSL 文件 - dake 数据包定义

# 设置变量
set(PROJECT_NAME, "example-project")
set(VERSION, "1.0.0")

# 设置环境变量
set.env(OUTPUT_DIR, "/tmp/dake-output")

# 定义数据包
lib
    name:"example-lib"
    version:"1.0.0"
    desc:"这是一个示例数据包"
    repo:"https://github.com/example/repo"
    keywords:["data", "package", "example"]
    readme:"README.md"
    out_dir:"./output"

# 简单的打印语句
PRINT("你好👋")
PRINT("开始处理数据包")

# 控制流示例
IF true:
    PRINT("条件为真")
ELSE:
    PRINT("条件为假")

# 文件操作
FILES(
    "data/file1.txt",
    "data/file2.txt",
    "${OUTPUT_DIR}/result.txt"
)
