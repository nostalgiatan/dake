# 把宽表收成另一种结构

目标是从多出来的列里留下需要的行和字段，再排序、分组。改目录里的文件见[按目录改写](rewrite-directory.md)。

来源 `person` 有 `name`、`email`、`plan`。要留下的 `member` 只有前两个字段。

```dsl
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
    plan: str

struct member:
    from: str
    layout: split
    sep: ","
    name: str
    email: str

struct by_plan:
    from: str
    layout: json
    plan: str
    people: list person

set(rows, files.rows("people.csv", person, header: true))
set(rows, list.where(${rows}, plan, "pro"))
set(rows, list.pick(${rows}, member))
set(rows, list.sort(${rows}, name))
set(grouped, list.group(${rows}, plan, by_plan))
```

`list.where` 按字段等值筛选。`list.pick` 丢掉结果结构里没有的字段。`list.sort` 默认升序，`order: "desc"` 为降序，同值保持原来的先后。`list.group` 的结果结构必须正好是组键，再加一个元素为来源结构的列表。组按第一次出现的顺序排列。

然后：

```bash
dake run reshape.dake
```
