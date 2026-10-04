# 让一个地址和目录持续分发

目标是给网络地址和目录各起一个名字，再按路径把请求和文件分到不同行为。检查脚本时，直线上必然失败的文件和目录会先被列出来。

```dsl
url api "127.0.0.1:8080"
dir inbox "./inbox" suffix: ".json"

struct person:
    from: str
    layout: json
    name: str

action answer(row, id):
    set(result, update(${row}, name, ${id}))

action store(row):
    set(result, ${row})

serve:
    route api "/people/{id}" person answer
    route inbox "/{name}.json" person store
    route inbox person store
```

`/people/ann` 命中第一条网络路由，`ann` 成为行为的第二个参数。同名文件 `ann.json` 命中目录里更具体的那条，其余文件走默认路由。一条坏请求或一份坏文件不会让 `serve` 停下来。

然后：

```bash
dake check serve.dake
dake run serve.dake
```

`check` 会报告不存在的 `inbox`，或一条顶层读取指向了还没有的文件。写在 `if` 或行为里的路径要等真正执行才判断。
