# dake

dake 是一门用于描述数据包的语言，以及执行这门语言的命令行程序。脚本写明包是什么、收进哪些文件、文件内容怎样读取和改写。`dake` 把脚本编译成中间表示再执行，不解释着源码往下跑。

当前版本是 0.6.0。用 Rust 编写。

## 语言

关键字全是小写，并且只在行首开始语句。写在路径或字段里时，它们仍是名字。块以冒号结尾，靠缩进结束。变量只通过 `${名字}` 读取。行为必须有名字，写成 `action 名字(参数):`。`use "相对路径" as 别名` 在编译时引入另一个脚本。`::` 是模块，`.` 是行为。

一条脚本可以定义 `lib`（包名、版本、输出目录）和 `repo`（容量与包数上限）。`files` 只收集路径。出现 `lib` 时，运行结束会在输出目录写下 `manifest.json` 和文件副本，可选加密。`files` 收集路径。`pack` 把记录列表按自身结构写进包，清单记下结构名。`unpack` 按清单里的结构把这项读回记录。`files.decry` 用 `dake.key` 把密文字节解回原来的字节。`await` 在写入的变量名不相交时并行执行。管道可以像行为一样声明参数。

文件本身不是值。读入之后才是字节或字符串，二者不能隐式转换。数字分整数和小数。`struct` 声明载体、切分和字段。字段可以带 `check`、`default` 和 `take`。结构头可以写 `replaces`，让旧包按新结构读入。记录用 `${doc}.字段` 读取，用 `update` 得到新记录，再用 `files.write` 写回。也可以用 `record(结构, 字段, 值, ...)` 直接造出一条记录。列表用 `[下标]` 取值，用 `[起:止]` 取一段，用 `list.set` 替换、`list.remove` 删掉一项。`object` 是没有结构的键值。多行文本可以用 `files.rows` 一次读成记录列表，用 `files.write.rows` 写回。`header: true` 读写分隔行文件的表头。`files.list` 按目录返回路径，不把它们收进包。`list.map` 和 `list.keep` 用一个行为处理列表里的每一项。`list.update` 给同一种记录的同一字段换成同一个值。`list.where` 按字段等值留下记录，`list.pick` 收成另一种结构，`list.sort` 排序，`list.group` 按字段分组。`list.join` 按字段把两份记录拼成第三种结构。`files.name` 和 `files.dir` 从路径取出文件名和目录。`net.url` 先定下基址和请求头，`net.get` 与 `net.post` 再按结构收发。地址是 `https` 时自动走 TLS。`net.accept` 只接一个明文请求。`url` 和 `dir` 给地址和目录命名，`serve` 里的 `route` 按路径持续分发。`dake check` 会先列出直线路径上必然发生的缺失和权限问题。`sys.host`、`sys.cpu`、`sys.mem`、`sys.disk` 和 `sys.disks` 读取这次运行所在机器的快照，不写入清单。`each` 仍可逐项处理并取出下标；`stop` 离开当前这一层循环。`catch as 变量:` 可以在失败时进入紧跟的 `else`。`pack` 与 `unpack` 可以带 `key`，只加密这一项。

## 命令

| 命令 | 作用 |
| --- | --- |
| `dake check FILE` | 解析、解析 `use`、降到中间表示。失败时退出码为 1 |
| `dake run FILE` | 走与 `check` 相同的编译，然后执行 |
| `dake ast FILE` | 打印语法树，不解析 `use`，不执行 |

## 文档

文档按用途分开。上面是项目本身，细节在这些篇里：

- [教程：做出第一个数据包](docs/tutorials/first-package.md)
- [操作指南](docs/how-to/README.md)
- [参考](docs/reference/README.md)
- [解释：路径与中间表示](docs/explanation/paths-and-ir.md)
- [解释：文件、载体与记录](docs/explanation/values.md)
