# 把文件打进加密包

脚本里先列出文件，再打开加密，并给出 `lib` 的输出目录。路径不要包含 `..`。

```dsl
lib:
    name: "sealed"
    version: "0.3.0"
    out_dir: "sealed-out"

files("data/note.txt")
files.encry()
```

```bash
dake run script.dake
```

`sealed-out` 里有 `manifest.json`、`note.txt.enc` 和 `dake.key`。清单里的 `encrypted` 为 true。

要排除某个目录名时，用当前目录收集并跳过它：

```dsl
files.all(exclude: ["temp"])
```
