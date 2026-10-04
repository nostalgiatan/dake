# 命令行

程序名：`dake`。

| 命令 | 作用 |
| --- | --- |
| `dake check FILE` | 读文件、做语法分析、解析 `use`、降到中间表示。失败时退出码为 1。 |
| `dake run FILE` | 在 `check` 的同一条编译路径之后执行中间表示。 |
| `dake run FILE --verbose` | 运行时打印读取路径和语句数。 |
| `dake ast FILE` | 打印语法树。不做 `use` 解析，不执行。 |

`FILE` 是脚本路径。
