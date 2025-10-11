# 完整 DSL 示例 - 展示主要语法特性

# ============ 变量定义 ============
set(PROJECT_NAME, "data-pipeline")
set(VERSION, "1.0.0")

# 环境变量
set.env(WORK_DIR, "/tmp/dake-work")
set.env(LOG_DIR, "/tmp/dake-logs")

# ============ 数据包定义 ============
lib
    name:"data-pipeline"
    version:"1.0.0"
    desc:"一个完整的数据处理管道示例"
    repo:"https://github.com/example/data-pipeline"
    keywords:["data", "pipeline", "etl", "processing"]
    readme:"README.md"
    out_dir:"./output"

# ============ 控制流示例 ============
PRINT("=== 控制流测试 ===")

IF true:
    PRINT("条件为真")
ELSE:
    PRINT("条件为假")

# ============ 文件加密 ============
PRINT("=== 文件加密 ===")
FILES.ENCRY()
PRINT("文件加密已启用")

# ============ 打印测试 ============
PRINT("=== 变量插值测试 ===")
PRINT("项目名称: ${PROJECT_NAME}")
PRINT("版本: ${VERSION}")
PRINT("工作目录: ${WORK_DIR}")
PRINT("日志目录: ${LOG_DIR}")

# ============ Emoji 支持 ============
PRINT("=== Emoji 测试 ===")
PRINT("✅ 完成")
PRINT("⚠️  警告")
PRINT("❌ 错误")
PRINT("🚀 启动")
PRINT("📦 打包")
PRINT("🔒 加密")
PRINT("👋 你好")

# ============ 结束 ============
PRINT("=== 程序结束 ===")

