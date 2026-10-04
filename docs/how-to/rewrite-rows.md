# 改写分隔行文件

目标是把多行文本里每一行的某一列改掉，再写回原路径。

文件 `people.csv`：

```csv
ann,a@b.co
bee,b@c.co
```

脚本：

```dsl
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str

action rename(row):
    set(result, update(${row}, name, "新"))

set(rows, files.rows("people.csv", person))
set(rows, list.map(${rows}, rename))
files.write.rows("people.csv", ${rows})
```

然后：

```bash
dake run rows.dake
```

字段个数与结构不一致、整数或小数无法解析，或布尔值不是 `true` / `false` 时，运行失败。字段内容里有分隔符、引号或换行时，用双引号包住该字段，引号本身写成两个双引号。

文件带表头时，读和写都加上 `header: true`。读入会核对该行与结构字段名一致，写回时先写字段名：

```dsl
set(rows, files.rows("people.csv", person, header: true))
set(rows, list.map(${rows}, rename))
files.write.rows("people.csv", ${rows}, header: true)
```

`files.read(路径, person)` 把整份文件当成一条记录，不按行拆开。多行用 `files.rows`。
