# 语言

脚本是 UTF-8 文本。关键字全为小写。语句由行首的关键字决定。同一个词出现在路径、字段、参数或命名参数里时是名字，例如 `${obj}.set`。块以冒号结尾，下一行起缩进必须大于块头，同一块内缩进相同。缩进只用空格。注释以 `#` 开始，直到行尾。

## 路径

`::` 分隔模块段。`.` 后面是行为名。

| 写法 | 含义 |
| --- | --- |
| `clean` | 当前文件里的行为 `clean` |
| `csv.clean` | 别名 `csv` 里的 `clean` |
| `tools::pack.seal` | 别名 `tools::pack` 里的 `seal` |

内置行为：`files`、`files.all`、`files.encry`、`files.decry`、`files.read`、`files.rows`、`files.list`、`files.name`、`files.dir`、`files.write`、`files.write.rows`、`files.read.bytes`、`files.read.str`、`files.write.bytes`、`files.write.str`、`base64.encode`、`base64.decode`、`text.lines`、`text.join`、`text.decode`、`text.encode`、`update`、`list.of`、`list.add`、`list.set`、`list.remove`、`list.map`、`list.keep`、`list.update`、`list.join`、`list.where`、`list.pick`、`list.sort`、`list.group`、`list.len`、`record`、`object`、`log.init`、`log.info`、`log.error`、`data.re`、`data.re.find`、`data.re.group`、`data.re.all`、`data.re.replace`、`utf8.encode`、`utf8.decode`、`data.vali`、`data.seria`、`data.deseria`、`data.comp`、`data.decomp`、`pack`、`unpack`、`net.url`、`net.get`、`net.post`、`net.accept`、`cmd`、`sys.host`、`sys.cpu`、`sys.mem`、`sys.disk`、`sys.disks`。语句 `url`、`dir`、`serve`、`route` 用于持续运行。

## 值

`${标识符}` 读取变量。裸标识符是路径或参数名。主表达式后可写 `[下标]` 与 `.字段`，二者可以交替。列表下标从 0 开始，必须是整数，越界是错误。对象可以用 `.字段`，也可以用字符串下标。`list.len(列表)` 返回长度。`list.set(列表, 下标, 值)` 返回替换后的新列表。`list.remove(列表, 下标)` 返回删掉该项后的新列表。`object(键, 值, ...)` 造出对象，键是字符串。

表达式：整数与小数 `+ - * /`，字符串 `+` 为拼接，比较 `== != > < >= <=`，`&&` `||` `!`，括号。整数与整数相加仍是整数。一侧是小数时结果是小数。`1.5` 是小数，点后必须是数字。`num(表达式)` 把整数、整数字符串，或有限且没有小数部分的小数转成整数。有小数部分的小数不能转成整数。`str(表达式)` 把值转成字符串。列表切片写成 `${列表}[起:止]`，起、止是整数，含起不含止，并且 `0 <= 起 <= 止 <= 长度`。不是列表或越界是错误。

行为调用可作为语句，也可作为 `set` 的右值。被调用的行为若执行了 `set(result, 值)`，该值成为调用结果；否则结果为布尔值 true。

## 语句

| 语句 | 形式 |
| --- | --- |
| 赋值 | `set(名字, 表达式)` |
| 环境变量 | `set.env(名字, 表达式)` |
| 引入 | `use "相对路径" as 模块路径` |
| 行为 | `action 名字(参数, ...):` 后接缩进块。名字必填。参数个数必须与调用一致。 |
| 管道 | `pipe 名字:` 或 `pipe 名字(参数, ...):`。后接每行一条路径，路径后可以有参数列表。步骤按顺序执行，参数与行为相同 |
| 条件 | `if 表达式:`、`elif 表达式:`、`else:` |
| 数据包 | `lib:` 字段 `name`、`version`、`desc`、`repo`、`keywords`、`readme`、`mods`、`out_dir`。`version` 是语义化版本字符串。`keywords` 与 `mods` 是字符串列表。 |
| 仓库 | `repo:` 字段 `name`、`capacity`（MiB）、`max_pkgs` |
| 打印 | `print(表达式)` |
| 执行 | `doing 路径` |
| 并发 | `await 路径, 路径, ...`。这些行为写入的变量名若相交，编译失败。各分支在独立线程上执行，可以读取文件、访问已设置的网络目标、调用其他行为。结构、错误说明、网络目标与加密状态在进入 `await` 时共享为快照。写入的变量、收进包的路径和 `pack` 的记录在全部分支结束后合并。 |
| 重复 | `each ${名字} in files:` 对本次收集的文件路径各执行一次。`each ${名字} at ${下标} in 表达式:` 的 `at` 可省略。来源必须是列表。`stop` 立刻离开当前这一层 `each`，这一轮剩下的语句不再执行。写在 `each` 外面是错误 |
| 错误说明 | `error 名字:` 下一行缩进一个字符串 |
| 处理 | `catch 名字 as 变量:` 或 `catch as 变量:`。名字可省略。进入块之前，有名字时把该错误的说明写入变量。块里失败时，变量改为这次失败的说明，并执行紧跟的 `else` 块。没有 `else` 时从块之后继续 |
| 调用 | `路径(参数, ...)`。命名参数写作 `名字: 表达式`。参数少于该行为或管道所声明的个数时，运行失败 |

`files(路径, ...)` 把已存在的文件收进包。`files.all(exclude: ["目录名", ...])` 从当前目录递归收集，跳过名称匹配的条目。`files.encry()` 使随后写出的文件成为密文，并在输出目录写入 `dake.key`。`files.decry(密钥路径, 密文字节)` 用该密钥文件里的十六进制密钥解回字节。

## 字节与文本

`files.read.bytes(路径)` 返回字节。`files.read.str(路径)` 按 UTF-8 读成字符串，非法字节是错误。`files.write.bytes(路径, 字节)` 与 `files.write.str(路径, 字符串)` 按对应类型写回。这四条不把文件收进包。路径拒绝 `..`。

字节不能与字符串相加，不能用 `str` 或 `num` 转换，也不能直接 `print`。`base64.encode(字节)` 返回不换行的标准 base64 字符串。`base64.decode(字符串)` 返回字节，非法字符是错误。`base64.encode` 不接受字符串。

## 结构

`struct` 声明载体、切分和字段。`from` 是 `str` 或 `bytes`。`layout` 是 `whole`、`lines`、`split`、`json` 或 `width`。

```dsl
struct note:
    from: str
    layout: lines
    title: str
    body: str
```

`whole` 只有一个字段，类型与 `from` 相同。`lines` 只适用于 `from: str`，并且正好两个 `str` 字段：第一行、其余行。`split` 只适用于 `from: str`，用结构头 `sep` 切开整段文本。`sep` 是非空字符串，不是字段。字段类型只能是 `str`、`int`、`float`、`bool`。字段含分隔符、引号或换行时用双引号包起来，引号本身写成两个双引号。`json` 只适用于 `from: str`，把 JSON 对象的键对应到字段。字段类型可以是 `str`、`int`、`float`、`bool`、`bytes`、`list` 或另一个结构。`float` 字段接受 JSON 小数，也接受 JSON 整数。`width` 只适用于 `from: bytes`。结构头 `order` 是 `be` 或 `le`，省略时是 `be`。结构头 `replaces: 旧结构名` 表示这个结构接替旧结构，旧结构名可以写成 `别名.结构名`。一个旧结构只能被替换一次。可以连着替换，出现环则编译失败。

`str` 字段可以写 `check: 验证器`，验证器是 `not_empty`、`email`、`url`、`numeric`、`alpha`、`alphanumeric`。`str`、`int`、`float`、`bool` 可以写 `default: 字面量`，类型必须与字段一致。带 `check` 的默认值在编译时就要通过检查。`take: 旧字段名` 表示替换时从旧结构的这个字段取值，不写则按同名取值。

造出或改出记录时都会跑 `check`：`record`、`text.decode`、`files.read`、`files.rows`、`files.write`、`files.write.rows`、`pack`、`unpack`、`update`、`list.update`、`list.pick`、`list.join`、`list.group`。失败信息带字段名。`record` 可以省掉有 `default` 的字段。

`unpack` 若见到被 `replaces` 的结构，先按旧结构的布局解码，再填进新结构。旧字段里没有、新字段也没有 `default` 的，运行失败。旧结构多出的字段丢掉。填完再跑 `check`。`pack` 写入的仍是当前记录自己的结构编号。`files.rows` 按当前结构读，不用 `default` 补列，也不走 `replaces`。

每个字段写成类型加宽度，例如 `magic: bytes 4`、`count: int 2`、`amount: float 4`。整数宽度是 1、2、4 或 8，有符号。小数宽度是 4 或 8。布尔值宽度是 1，字节是 0 或 1。字节字段的长度必须等于宽度。最后一个字段可以是不写宽度的 `bytes`，表示剩下的全部字节。没有这个字段时，整段长度必须等于各字段宽度之和。

```dsl
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
```

`text.decode(文本, person)` 得到记录。`text.encode(记录)` 按该记录的分隔符拼回字符串。`list.add(列表, 值)` 返回新列表。`list.of()` 可以没有元素。`list.len(列表)` 返回整数。`list.map(列表, 行为)` 对每一项调用该行为，行为只能有一个参数，并且必须 `set(result, 值)`，返回这些值组成的新列表。`list.keep(列表, 行为)` 同样调用，`result` 为真时留下原来的项。`list.update(列表, 字段, 值)` 对每条记录替换该字段，返回新列表。记录必须是同一种结构，字段必须存在，值的类型必须相符。`list.where(列表, 字段, 值)` 留下该字段与值相等的记录。`list.pick(列表, 结构)` 按结果结构的字段名从来源复制，丢掉来源多出的字段。结果字段在来源中没有，或类型不符，运行失败。`list.sort(列表, 字段)` 按该字段升序，同值保持原来的先后。`order: "desc"` 为降序。字段类型只能是 `str`、`int`、`float`、`bool`。`list.group(列表, 字段, 结果结构)` 按字段值分组。组的顺序是第一次出现的顺序，组内保持原顺序。结果结构必须正好是该字段再加一个 `list` 字段，列表元素是来源那种结构。组键类型不符，或多出别的字段，运行失败。以上四条在空列表时得到空列表。`list.join(左列表, 左字段, 右列表, 右字段, 结果结构)` 留下两边字段值相等的行，按结果结构的字段名从两边填入。两边都有且值不同、两边都没有、类型不符，或连接字段的类型不同，运行失败。任一边为空时得到空列表。`files.rows(路径, 结构)` 把文本文件的每一行解码成记录并返回列表，结构必须是 `split` 或 `json`。空文件得到空列表。`header: true` 时第一行必须是该结构的字段名，顺序与声明一致，返回值不含这一行；名字不符或文件为空则失败。`header` 只适用于 `split`。`files.write.rows(路径, 列表)` 把同一种记录按行写回。`header: true` 先写字段名，`header: "一行"` 把该字符串原样写在第一行。`append: true` 接在已有内容之后；文件不存在则创建；已有内容末尾不是换行时先补一个换行。`append` 与 `header` 同时给出则失败。省略 `append` 时整份覆盖。

`files.list(目录)` 返回这一层的文件路径，不把它们收进包。`suffix: ".csv"` 只保留文件名以此结尾的项。`deep: true` 进入子目录。`exclude: ["目录名"]` 在递归时跳过这些目录名。`files.name(路径)` 返回最后一段文件名，没有文件名则失败。`files.dir(路径)` 返回所在目录，只有文件名时返回 `.`。二者不读文件，也不收进包。路径里出现 `..` 是错误。

## 网络

`net.url(基址)` 记下当前目标。基址必须是 `http` 或 `https`。之后 `net.get` 和 `net.post` 可以不写地址。以 `/` 开头的路径接在基址已有的路径后面。带方案的地址只用于这一次，不改当前目标。`name: "api"` 再记一个具名目标，调用时用 `url: "api"` 选用。没有目标又没写地址则失败。

`headers` 是一条记录。字段名里的 `_` 发出去变成 `-`，每段首字母大写，`content_type` 成为 `Content-Type`。值必须是字符串。调用上再写 `headers` 时，同名覆盖，其余留下。没有 `Content-Type` 时由正文补上：`json` 是 `application/json`，字节或 `width` 是 `application/octet-stream`，其余文本是 `text/plain; charset=utf-8`。

协议不单独作为参数。`http` 是明文，`https` 是 TLS，主机名用于 SNI，证书必须受信任且主机名相符。`ca` 给出 PEM 时只用其中的证书做信任锚。`cert` 与 `key` 必须成对，是客户端证书和私钥。`timeout` 是秒数，省略为 30。这些都写在 `net.url` 上。

`net.get()` 返回字节。`net.get(结构)` 按该结构解码成一条记录，规则与 `files.read` 相同，并跑 `check`。`net.post(正文)` 发送字节、字符串或记录。记录按其自身结构编码。`net.post(正文, 结构)` 再解码响应。路径可以写在正文前面。响应不收进包。状态码不在 200 到 299 时失败。

网络错误码：`4030` 地址，`4031` 连接，`4032` TLS，`4033` 状态码，`4034` 请求头或超时。正文对不上结构时仍用结构自己的错误码。

`net.accept(地址, 结构, 行为)` 只接一个明文请求后返回。地址写成 `主机:端口`。请求体按结构解码成一条记录，传给只有一个参数的行为。行为必须 `set(result, 记录或字节)`，这就是响应体，也是 `net.accept` 的返回值。不做路由、并发连接和 TLS 监听。请求头这一版不读成记录。

## 持续运行

`url 名字 "主机:端口"` 和 `dir 名字 "目录"` 各给一类地址一个标识符。名字不能重复。`suffix`、`deep`、`exclude` 写在 `dir` 上，含义与 `files.list` 相同。

`serve:` 写在准备之后，块里只能是 `route`，后面不能再写语句。`route 名字 路径 结构 行为` 把事件分到行为。目录路由可以不写路径，那就是该目录的默认路由，每个 `dir` 最多一条。

路径按 `/` 分段。一段可以是字面量、`{名字}`、`*` 或末尾的 `**`。`{名字}.json` 这样的一段会接住后缀之前的文本。`{**名字}` 接住剩下的多段，用 `/` 拼成一个字符串。接住的名字按从左到右成为行为里记录参数之后的字符串参数。`.` 和 `..` 不会被参数接住。

能接住同一条路径时，从左到右比特异度：字面量高于带后缀的参数，带后缀的参数高于 `{名字}`，`{名字}` 高于 `*`，`*` 高于 `**`。比完仍相同则编译失败。

网络没有匹配时返回 404，不读正文。非法百分号返回 400。行为失败或响应不是记录或字节时返回 500，服务继续。目录没有匹配且没有默认路由时跳过该文件。文件要等大小和修改时间稳定后才读取。

启动时先按目录声明顺序、再按相对路径处理已有文件，然后才处理新到达的请求和文件。同一时刻只处理一件。`stop`、SIGINT 或 SIGTERM 结束整个 `serve`，当前这件做完后写出数据包。`net.accept` 仍只接一个明文请求；持续接请求用 `serve`。

## 宿主

`sys.host()`、`sys.cpu()`、`sys.mem()`、`sys.disks()` 不带参数。`sys.disk(路径)` 要一个已存在的路径。每次调用采样一次，返回没有结构的对象。`sys.disks()` 返回对象列表。

`sys.host()` 的字段 `name`、`os`、`version`、`kernel` 是字符串。`sys.cpu()` 的 `usage` 是小数百分比，`logical` 和 `physical` 是整数。`sys.mem()` 的 `used`、`total`、`swap_used`、`swap_total` 是字节整数。`sys.disk` 与列表中的每一项有 `name`、`mount`、`free`、`total`；`free` 和 `total` 是字节整数，`sys.disk` 的 `mount` 是该路径所在文件系统。路径不存在时失败码是 `3002`。这些数不写入清单。

## 预测试

`dake check` 和 `dake run` 在执行前检查顶层直线上的字面路径。写在 `if`、`each`、`catch`、行为或 `serve` 里的路径不提前猜测。同一路径先写入再读取，读取不算缺失。读取要求文件存在且可读。写入要求父目录存在且可写；文件尚不存在不算失败。目录必须存在且能列出。失败码是 `4037`。端口是否能绑上留到进入 `serve` 时再看。

`record(结构名, 字段名, 值, ...)` 不读文件，造出该结构的一条记录。字段名是标识符。没有 `default` 的字段必须给出，有 `default` 的可以省略。值的类型必须相符。`list` 和嵌套结构字段用列表和另一条记录填入。

`files.read(路径, note)` 返回记录。字段写成 `${doc}.title`。`set(doc, update(${doc}, title, "新标题"))` 得到一份新记录。`files.write(路径, ${doc})` 按记录自己的结构写回。结构名可经 `use` 写成 `别名.结构名`。

`text.lines(字符串)` 返回字符串列表。`text.join(列表, 分隔字符串)` 把列表拼回字符串。二者不读磁盘。

`out_dir` 若以 `./` 开头，编译时去掉该前缀，重复的 `./` 同样去掉。空路径、空字节、出现 `..` 的路径，以及 `foo/./bar` 这样的 `.` 段都是错误。`use`、`dir` 和运行时读写使用同一套检查。符号链接的目标必须是相对路径，且不能含有 `..` 或单独的 `.` 段。绝对目标会被拒绝。

程序结束时，若出现过 `lib`，在 `out_dir` 写入 `manifest.json` 和文件副本。清单字段：`name`、`version`、`desc`、`repo`、`keywords`、`readme`、`encrypted`、`files`（`name`、`bytes`、`blake3`、`struct`、`encrypted`）。`files` 收进的副本，`struct` 为空。`pack(包内名字, 记录列表)` 把同一种 `split` 或 `json` 记录按行编码后收进包，`struct` 是该结构的编号。`key: "密钥路径"` 用该文件里的 32 字节十六进制密钥先加密这一项，清单该项 `encrypted` 为 true，包内名字不变。密钥文件不存在时写入新密钥。`unpack(路径)` 读取该文件。同一次运行里若刚 `pack` 过这个名字，直接用那份记录；否则读取该文件所在目录的 `manifest.json`，用对应项的 `struct` 按行解码成记录列表。该项已加密时必须带同一把 `key`，先解密再解码；没写 `key` 则失败。`files.encry()` 仍加密其余项，已经用 `key` 加密的项不再包第二层。编号能在当前脚本里找到就用它；否则按结构短名匹配，必须恰好一个。`struct` 为空、清单里没有这个名字、或文件不是 UTF-8 时失败。空列表、混有别的结构、或没有 `lib` 时 `pack` 失败。`files.read` 不把文件收进包。若出现过 `repo`，文件数超过 `max_pkgs` 或总字节超过 `capacity` MiB 时不写入。

`data.re(模式)` 编译正则并返回模式。`data.re.find(模式, 文本)` 返回第一处匹配，没有匹配是错误。`data.re.group(模式, 文本, 编号)` 返回捕获组，编号 0 是整段匹配。`data.re.all(模式, 文本)` 返回全部匹配的字符串列表。`data.re.replace(模式, 文本, 替换)` 替换全部匹配。`utf8.encode(字符串)` 返回 UTF-8 字节。`utf8.decode(字节)` 返回字符串，非法 UTF-8 是错误。`data.vali` 的第一个参数是验证器名：`not_empty`、`email`、`url`、`numeric`、`alpha`、`alphanumeric`。其后可以是字段名和值，或只有一个字符串值。`data.seria` 与 `data.deseria` 的第一个参数是 `json` 或 `bin`。`json` 进出的是字符串，`bin` 进出的是字节。反序列化得到布尔值、整数、小数、字符串、列表或对象。JSON 对象成为对象值，不要求事先声明结构。`data.comp` 接受字节或字符串，返回压缩后的字节；也可先给压缩级别。`data.decomp` 接受字节并返回字节。`cmd` 的第一个参数是可执行文件，其余是参数。

失败时输出三部分：错误码和说明、`位置: 文件:行:列`、`建议:` 里的改法。解析、编译和执行使用同一套格式。`check` 不通过时，建议是改字段值或去掉这项检查。`replaces` 重复或成环时，建议是只保留一条替换链。

调用、压缩、序列化、命令和日志的结果只存在于 `set` 写入的名字，或调用的返回值中。
