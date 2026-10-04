## Learned User Preferences

- Discuss language design in Chinese and review whether the grammar is correct and complete before large syntax changes.
- Import other script files with `use`, not `import`. Paths use `::` for modules and `.` for behaviors.
- Scripts should be compiled to an intermediate representation before execution, and defined behaviors must be named so pipelines can call them.
- After language changes, add language tests and run them before treating the work as done.
- Keep project docs in the Diátaxis layout under `docs/` (tutorials, how-to, reference, explanation), and update them when the language changes. Do not leave scratch reports in the repo root.

## Learned Workspace Facts

- `dake` is a Rust DSL packaging tool: scripts describe data packages, then the CLI (`check`, `ast`, `run`) validates, compresses, and encrypts them. Scripts also read and write structured file contents as bytes, text, lists, and records.
- The main program is split into `dsl` (lexer, parser, AST), `executor`, `data`, and `cli`. The `error` crate is what `dake` actually depends on; `rstream`, `sys`, and `transaction` are side libraries.
- Current syntax is one lowercase-keyword language: blocks end by indentation after a colon, variables are read only as `${name}`, and behaviors are `action name(params):`. Values include integers, decimals, strings, bytes, lists, records, and objects that have no struct.
- `dake run` parses, resolves `use "file" as alias` at compile time, lowers to IR, then executes. Missing or duplicate imports are compile errors. `await` runs in parallel with smol when the names written do not overlap.
- `files.rows` and `files.write.rows` read and write one `split` or `json` record per line. `list.map` and `list.keep` call a one-parameter action that must `set(result, ...)`.
- `pack(name, records)` encodes rows of one struct into the package and stores that struct id on the manifest item. Copies taken by `files` leave `struct` empty. `pack` requires `lib`. `unpack(path)` reads those rows back from the in-memory pack or from `manifest.json` beside the file.
- `files.decry(key_path, ciphertext_bytes)` decrypts package bytes with the hex key in `dake.key`. The result is bytes.
- `net.url` sets the current `http` or `https` target; `net.get` and `net.post` use it and return bytes or one decoded record. `net.accept` handles a single plaintext request. Network failures use the same error format.
- Parse, compile, and run failures use one format: the existing explanation, `位置: file:line:column`, and a `建议:` repair hint.
