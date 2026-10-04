# 引入另一个文件里的行为

把规则放在脚本旁边的文件里，例如 `rules/greet.dake`：

```dsl
action hi():
    print("hi")
```

在主脚本里：

```dsl
use "rules/greet.dake" as tools::pack

tools::pack.hi()
```

`use` 的路径相对主脚本，不能包含 `..`。别名用 `::` 分段。调用写成 `别名.行为`。

然后：

```bash
dake check main.dake
dake run main.dake
```

找不到文件或别名下没有该行为时，`check` 失败。
