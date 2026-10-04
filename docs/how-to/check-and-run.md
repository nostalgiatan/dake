# 检查并运行脚本

在脚本所在目录，或对脚本使用路径：

```bash
dake check path/to/script.dake
dake run path/to/script.dake
```

需要编译过程的行数时：

```bash
dake run path/to/script.dake --verbose
```

`check` 失败时先改脚本再运行。`check` 会做语法分析、解析 `use`，并降到中间表示。只看语法树时：

```bash
dake ast path/to/script.dake
```
