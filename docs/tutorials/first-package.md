# 做出第一个数据包

这一课结束时，你会得到目录 `lesson-out`，里面有 `manifest.json`。

## 构建 dake

在本仓库根目录执行：

```bash
cargo build
```

可执行文件是 `target/debug/dake`。下面的命令都在仓库根目录运行。

## 写脚本

创建文件 `lesson.dake`，内容如下：

```dsl
set(name, "lesson")

print("hello ${name}")

lib:
    name: "lesson"
    version: "0.1.0"
    desc: "第一课"
    out_dir: "lesson-out"
```

## 检查

```bash
target/debug/dake check lesson.dake
```

你会看到「检查通过」，以及语句数。

## 运行

```bash
target/debug/dake run lesson.dake
```

终端打印 `hello lesson`。打开 `lesson-out/manifest.json`，里面的 `name` 是 `lesson`。
