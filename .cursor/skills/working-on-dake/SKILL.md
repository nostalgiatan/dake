---
name: working-on-dake
description: >-
  Use when changing the dake language, executor, tests, or docs in this
  repository. Use when the user discusses 语法, 仓库, serve, 清单, lib, or
  asks to 更新文档.
---

# 在 dake 仓库里改代码和文档

语法、执行和文档是三件分开的事。改语法之前先把文法说完整。改完语言再补测试。文档按读者要做的事分开放，不把说明写进操作步骤。

## 何时使用

- 改 `src/dsl`、`src/executor`、`src/lang_tests.rs`，或 `docs/`
- 用户用中文讨论语法、仓库、协议、清单
- 用户给出书面计划并要求按计划实现

不用于：只问概念、不改这个仓库；或改 `src/crates` 里的 `rstream`、`sys`、`transaction`（执行器不依赖它们）。

## 语法

用中文把文法说清楚，并核对是否完整，再改解析器。关键字全小写。块在冒号后靠缩进结束。变量只通过 `${名字}` 读取。引入用 `use`，不用 `import`。`::` 是模块，`.` 是行为或字段。行为必须有名字，写成 `action 名字(参数):`，这样管道才能调用。

脚本先解析，编译期解析 `use`，降到中间表示，再执行。`dake check`、`dake ast`、`dake run` 走这条路径。

## 做完语言改动

在 `src/lang_tests.rs` 加测试。先跑一条过滤：

```bash
cargo test --offline --bin dake <过滤名>
```

过滤名对上新测试之后，再跑不带过滤的 `cargo test --offline --bin dake`。不要用 `--lib` 代替 `--bin dake`。

失败信息保持三行：说明、`位置: 文件:行:列`、`建议:`。新说明避开已被更早规则接住的词，例如单独的「找不到」「参数」。给 `src/dsl/diagnose.rs` 补上对应的 `建议:`。

## 文档

`docs/` 按 Diátaxis 分四类。一篇只属于一类。语言变了就改对应的那一篇，不在仓库根目录留草稿。

| 目录 | 读者要的 | 写法 |
| --- | --- | --- |
| `docs/tutorials/` | 跟着做一遍学会 | 连续步骤，少解释 |
| `docs/how-to/` | 做成一件具体的事 | 步骤和条件，不讲原理 |
| `docs/reference/` | 查字段、路径、状态码 | 表和事实，不写步骤，不写原因 |
| `docs/explanation/` | 理解为什么这样分 | 散文，不写操作清单 |

仓库的字段和 `/dake/v1` 写在 `docs/reference/language.md` 的「仓库」。挂到地址上的步骤写在 `docs/how-to/serve-repo.md`。只存放、不代核对的原因写在 `docs/explanation/repo.md`。

## 不要做

- 实现书面计划时不要改计划文件。
- 不要把宿主快照 `sys.*` 写进清单。
- 不要让仓库或协议调用 `files.unseal`、`files.verify`。`dake.key` 不进仓库，协议不收也不发。
- 提交时不要加入 `.cursor/hooks/` 和 `output/`。

## 常见错法

| 错法 | 改法 |
| --- | --- |
| 在 how-to 里解释为什么不收密钥 | 步骤留在 how-to，原因放到 `docs/explanation/` |
| 在 reference 里写「先运行再取回」 | 规格留在 reference，步骤放到 `docs/how-to/` |
| 只跑通主路径就说测试完整 | 先写会失败的语言测试，再跑上面的 cargo 命令 |
| 语法还没说完就开始改 parser | 先用中文核对文法，再改代码 |
