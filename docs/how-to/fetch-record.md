# 从网络读入一条记录

目标是先定下服务地址，再按结构读回一条记录。改本地文件见[改写分隔行文件](rewrite-rows.md)。

```dsl
struct person:
    from: str
    layout: json
    name: str
    email: str check: email

struct auth:
    from: str
    layout: json
    authorization: str

net.url("https://api.example/v1", headers: record(auth, authorization, "Bearer t"))
set(row, net.get(person))
set(row, net.get("/people/1", person))
```

`net.url` 记下基址和请求头。后面的 `net.get` 可以不写地址。以 `/` 开头的路径接在基址后面。地址写成 `https` 时自动使用 TLS。私有证书颁发机构用 `ca` 指向它的 PEM，调用仍然是 `net.get(person)`。

然后：

```bash
dake run fetch.dake
```
