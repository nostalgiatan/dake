# 把旧包读成新结构

目标是结构改了之后，已经打进包的旧数据仍能按新结构读出来。改当前文件见[改写分隔行文件](rewrite-rows.md)。

旧结构留在脚本里。新结构用 `replaces` 指向它。同名字段直接搬过来。名字变了就写 `take`。旧数据里没有的字段写 `default`。`check` 在读入之后仍然要过。

```dsl
struct person_v1:
    from: str
    layout: split
    sep: ","
    name: str
    mail: str

struct person:
    from: str
    layout: split
    sep: ","
    replaces: person_v1
    name: str check: not_empty
    email: str check: email take: mail
    plan: str default: "free"

set(rows, unpack("out/people.csv"))
```

`unpack` 先按 `person_v1` 的布局解码，再填成 `person`。`pack` 写进去的编号是当时那条记录自己的结构。一个旧结构只能被一个新结构替换。

然后：

```bash
dake run evolve.dake
```
