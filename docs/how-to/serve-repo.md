# 把仓库挂到已有地址

把一份已经能写出的包放进本机目录，再挂到已经声明的 `url` 上。另一份脚本按名字和版本取回，然后自己核对公钥。

先写出包并放入仓库：

```dsl
lib:
    name: "box"
    version: "0.3.0"
    out_dir: "./output"

repo:
    name: "desk"
    dir: "./desk"

files("./a.txt")
```

```bash
dake run pack.dake
```

`./output` 是作者目录。`./desk/box/0.3.0` 是已提交的那一份。

再挂到地址。`cert` 与 `key` 只在这条 `url` 上：

```dsl
repo:
    name: "desk"
    dir: "./desk"

url api "127.0.0.1:8080" cert: "cert.pem" key: "key.pem"

serve:
    repo api
```

```bash
dake run serve.dake
```

取回 `0.3.0`。目标目录 `box-out` 必须还不存在。信任锚写在 `net.url` 的 `ca` 上：

```dsl
net.url("https://127.0.0.1:8080", ca: "cert.pem")
repo.fetch("https://127.0.0.1:8080", "box", "0.3.0", "box-out")
files.unseal("box-out", "public.key")
```

清单里还有别的 `"名字 版本"` 时，对每一对再调用一次 `repo.fetch`。公钥签名要在包目录已经存在之后，对 `out_dir` 或 `repo.get` 的结果调用 `files.seal`。

字段、路径和状态码见[语言参考](../reference/language.md)里的「仓库」。仓库只存放、不代核对的原因见[仓库与协议](../explanation/repo.md)。
