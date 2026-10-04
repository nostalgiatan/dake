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

要声明依赖、替换哪一版，并给清单签名：

```dsl
lib:
    name: "sealed"
    version: "0.3.0"
    out_dir: "sealed-out"
    depends: ["base 1.0.0"]
    replaces: "sealed 0.2.0"
    sign: "sign.key"
```

`sign.key` 是 32 字节或 64 位十六进制，不会抄进输出目录。`files.seal` 用的私钥可以用 `crypto.pair("private.pkcs8", "public.key")` 生成。两个文件都不能已经存在。之后用 `files.verify("sealed-out", key: "sign.key", depend: ["base-out"], replace: "sealed-0.2")` 重算哈希，并打开依赖目录和被替换的目录，核对名字、版本和签名。`sign` 是共享密钥。要让别人只核对、不能改签，用另一对函数：`files.seal("sealed-out", "private.pkcs8")` 把公钥写成 `dake.pub`、签名写成 `dake.seal`，私钥留在原地。`files.unseal("sealed-out", "public.key")` 用这份公钥核对清单，并按块重算包内文件的哈希。`files.verify` 也按块重算。

要排除某个目录名时，用当前目录收集并跳过它：

```dsl
files.all(exclude: ["temp"])
```
