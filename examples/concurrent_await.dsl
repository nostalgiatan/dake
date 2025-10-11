# 并发执行示例
#
# 演示使用 smol 运行时实现的真正并发 AWAIT 功能
# 注意：此示例展示概念，实际运行需要先定义 DATA.PIPE

# 设置变量
set(TASK_COUNT, "5")

PRINT("=== 并发执行演示 ===")
PRINT("本示例演示 AWAIT 并发功能")
PRINT("")
PRINT("AWAIT 语法使用 smol 异步运行时实现真正的并发执行。")
PRINT("所有引用的操作将在独立的任务中并发执行，而不是顺序执行。")
PRINT("")
PRINT("优势：")
PRINT("  ✅ 真正的并发：使用 smol 异步运行时")
PRINT("  ✅ 性能优化：单引用自动优化为顺序执行")
PRINT("  ✅ 类型支持：支持 DATA.PIPE 和 COMM.ACTION")
PRINT("  ✅ 错误处理：完整的错误收集和报告")
PRINT("")
PRINT("示例语法：")
PRINT("  AWAIT(")
PRINT("      DATA.PIPE.preprocessing,")
PRINT("      DATA.PIPE.validation,")
PRINT("      DATA.PIPE.transformation")
PRINT("  )")
PRINT("")
PRINT("已完成 ${TASK_COUNT} 个任务的并发处理!")


