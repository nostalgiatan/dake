# 按目录改写

目标是改写一个目录里某一种文件，而不是事先写死每一条路径。

`files.list` 只返回路径。这些路径不会出现在数据包清单里。

```dsl
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str

each ${path} in files.list("data", suffix: ".csv", deep: true):
    set(rows, files.rows(${path}, person, header: true))
    set(rows, list.update(${rows}, name, "新"))
    files.write.rows(${path}, ${rows}, header: true)
```

`suffix` 比较的是文件名结尾。省略则不过滤。`deep` 省略或为 false 时只看这一层。为 true 时进入子目录，`exclude` 列出要跳过的目录名。

然后：

```bash
dake run rewrite-dir.dake
```

写到旁边的目录时，用文件名拼出新路径：

```dsl
files.write.rows(text.join(["out/", files.name(${path})], ""), ${rows}, header: true)
```
