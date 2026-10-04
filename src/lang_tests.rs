/*
 * 语言级测试：每个用例单独落盘，走 resolve、lower、execute_ir。
 */

use crate::dsl::ir::{lower, Inst};
use crate::dsl::resolve::resolve_file;
use crate::executor::Executor;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static NEXT: AtomicU64 = AtomicU64::new(0);
static CWD: Mutex<()> = Mutex::new(());

struct Ran {
    output: Vec<String>,
    error: Option<String>,
}

fn write_script(name: &str, src: &str) -> std::path::PathBuf {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("dake_lang_{name}_{id}.dake"));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(src.as_bytes()).unwrap();
    path
}

fn compile(src: &str) -> Result<crate::dsl::ir::Program, String> {
    let path = write_script("compile", src);
    let image = resolve_file(&path)?;
    lower(&image)
}

fn run(src: &str) -> Ran {
    let path = write_script("run", src);
    let image = match resolve_file(&path) {
        Ok(image) => image,
        Err(e) => return Ran { output: Vec::new(), error: Some(e) },
    };
    let program = match lower(&image) {
        Ok(program) => program,
        Err(e) => {
            let e = if e.contains("建议:") {
                e
            } else {
                format!("{e}\n  建议: {}", crate::dsl::diagnose::hint(0, &e))
            };
            return Ran { output: Vec::new(), error: Some(e) };
        }
    };
    let mut executor = Executor::new();
    let error = executor.execute_ir(&program).err().map(|e| e.to_string());
    Ran { output: executor.output().to_vec(), error }
}

fn assert_ok(src: &str) -> Vec<String> {
    let ran = run(src);
    assert!(ran.error.is_none(), "脚本失败: {:?}\n脚本:\n{src}", ran.error);
    ran.output
}

fn assert_err(src: &str, needle: &str) {
    let ran = run(src);
    let message = ran.error.unwrap_or_else(|| format!("应当失败，输出 {:?}", ran.output));
    assert!(message.contains(needle), "错误信息 {message} 不包含 {needle}\n脚本:\n{src}");
}

#[test]
fn arith_concat_and_casts() {
    let output = assert_ok("set(n, 1 + 2 * 3)\nprint(str(${n}))\nset(s, \"a\" + \"b\")\nprint(${s})\nset(k, num(\"4\") + 1)\nprint(str(${k}))\n");
    assert!(output.iter().any(|l| l == "7"), "{output:?}");
    assert!(output.iter().any(|l| l == "ab"), "{output:?}");
    assert!(output.iter().any(|l| l == "5"), "{output:?}");
    assert_err("set(x, \"a\" + 1)\n", "类型不同");
}

#[test]
fn if_does_not_swallow_following_statement() {
    let output = assert_ok("if true:\n    print(\"in\")\nprint(\"after\")\n");
    assert_eq!(output, vec!["in".to_string(), "after".to_string()]);
}

#[test]
fn call_result_is_named() {
    let src = "action twice():\n    set(result, 2 * 3)\nset(out, twice())\nprint(str(${out}))\n";
    let output = assert_ok(src);
    assert!(output.iter().any(|l| l == "6"), "{output:?}");
}

#[test]
fn pipe_without_params_runs_and_missing_params_fail() {
    let output = assert_ok("action ping():\n    print(\"pong\")\npipe process:\n    ping\ndoing process\n");
    assert!(output.iter().any(|l| l == "pong"), "{output:?}");
    assert_err("action clean(file):\n    print(${file})\ndoing clean\n", "参数数量不符");
}

#[test]
fn crypto_pair_writes_a_new_key() {
    let dir = std::env::temp_dir().join(format!("dake_pair_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let private = dir.join("private.pkcs8");
    let public = dir.join("public.key");
    let src = format!("crypto.pair(\"{}\", \"{}\")\nprint(\"made\")\n", private.display(), public.display());
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "made"), "{output:?}");
    assert_eq!(std::fs::read(&public).unwrap().len(), 32);
    assert_err(&src, "密钥文件已存在");
    assert_err(&format!("crypto.pair(\"{}\", \"{}\")\n", private.display(), private.display()), "同一个文件");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_manifest_and_each() {
    let dir = std::env::temp_dir().join(format!("dake_lang_files_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let out = std::env::temp_dir().join(format!("dake_lang_out_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"aaa").unwrap();
    std::fs::write(dir.join("b.txt"), b"bbb").unwrap();
    let key_path = dir.join("sign.key");
    std::fs::write(&key_path, "11".repeat(32)).unwrap();
    let src = format!(
        "lib:\n    name: \"box\"\n    version: \"0.3.0\"\n    out_dir: \"{}\"\n    depends: [\"base 1.0.0\"]\n    replaces: \"box 0.2.0\"\n    sign: \"{}\"\nfiles(\"{}/a.txt\", \"{}/b.txt\")\neach ${{file}} in files:\n    print(${{file}})\n",
        out.display(),
        key_path.display(),
        dir.display(),
        dir.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l.contains("a.txt")), "{output:?}");
    assert!(output.iter().any(|l| l.contains("b.txt")), "{output:?}");
    let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    assert!(manifest.contains("a.txt"));
    assert!(manifest.contains("b.txt"));
    assert!(manifest.contains("\"depends\": [\"base 1.0.0\"]"), "{manifest}");
    assert!(manifest.contains("\"replaces\": \"box 0.2.0\""), "{manifest}");
    assert!(!manifest.contains("sign.key"), "{manifest}");
    let base = std::env::temp_dir().join(format!("dake_lang_base_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("c.txt"), b"ccc").unwrap();
    let base_out = std::env::temp_dir().join(format!("dake_lang_base_out_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&base_out);
    let base_src = format!(
        "lib:\n    name: \"base\"\n    version: \"1.0.0\"\n    out_dir: \"{}\"\n    sign: \"{}\"\nfiles(\"{}/c.txt\")\n",
        base_out.display(),
        key_path.display(),
        base.display()
    );
    assert_ok(&base_src);
    let old_out = std::env::temp_dir().join(format!("dake_lang_old_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&old_out);
    let old_src = format!(
        "lib:\n    name: \"box\"\n    version: \"0.2.0\"\n    out_dir: \"{}\"\n    sign: \"{}\"\nfiles(\"{}/a.txt\")\n",
        old_out.display(),
        key_path.display(),
        dir.display()
    );
    assert_ok(&old_src);
    let verify = format!(
        "files.verify(\"{}\", key: \"{}\", depend: [\"{}\"], replace: \"{}\")\nprint(\"ok\")\n",
        out.display(),
        key_path.display(),
        base_out.display(),
        old_out.display()
    );
    let checked = assert_ok(&verify);
    assert!(checked.iter().any(|line| line == "ok"), "{checked:?}");
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let private = dir.join("private.pkcs8");
    std::fs::write(&private, pkcs8.as_ref()).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let public = dir.join("public.key");
    std::fs::write(&public, ring::signature::KeyPair::public_key(&pair).as_ref()).unwrap();
    let seal = format!("files.seal(\"{}\", \"{}\")\nfiles.unseal(\"{}\", \"{}\")\nprint(\"sealed\")\n", out.display(), private.display(), out.display(), public.display());
    let sealed = assert_ok(&seal);
    assert!(sealed.iter().any(|line| line == "sealed"), "{sealed:?}");
    assert_eq!(std::fs::read(out.join("dake.pub")).unwrap().len(), 32);
    let saved = std::fs::read(out.join("manifest.json")).unwrap();
    std::fs::write(out.join("manifest.json"), b"{}").unwrap();
    assert_err(&format!("files.unseal(\"{}\", \"{}\")\n", out.display(), public.display()), "公钥签名不符");
    std::fs::write(out.join("manifest.json"), saved).unwrap();
    let original = std::fs::read(out.join("a.txt")).unwrap();
    std::fs::write(out.join("a.txt"), b"zzz").unwrap();
    assert_err(&format!("files.unseal(\"{}\", \"{}\")\n", out.display(), public.display()), "哈希不符");
    std::fs::write(out.join("a.txt"), original).unwrap();
    std::fs::write(out.join("a.txt"), b"zzz").unwrap();
    assert_err(&verify, "哈希不符");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn files_all_skips_excluded_dir() {
    let _guard = CWD.lock().unwrap();
    let root = std::env::temp_dir().join(format!("dake_lang_all_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("keep")).unwrap();
    std::fs::create_dir_all(root.join("skip")).unwrap();
    std::fs::write(root.join("keep/a.txt"), b"a").unwrap();
    std::fs::write(root.join("skip/b.txt"), b"b").unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    let output = assert_ok("files.all(exclude: [\"skip\"])\neach ${file} in files:\n    print(${file})\n");
    std::env::set_current_dir(previous).unwrap();
    let joined = output.join("\n");
    assert!(joined.contains("a.txt"), "{output:?}");
    assert!(!joined.contains("b.txt"), "{output:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn out_dir_dotdot_rejected_and_dot_slash_stripped() {
    assert!(compile("lib:\n    name: \"x\"\n    version: \"0.1.0\"\n    out_dir: \"../out\"\n")
        .unwrap_err()
        .contains("不安全"));
    let program = compile("lib:\n    name: \"x\"\n    version: \"0.1.0\"\n    out_dir: \"./output\"\n").unwrap();
    let found = program.insts.iter().any(|inst| matches!(inst, Inst::Lib(lib) if lib.out_dir == "output"));
    assert!(found);
}

#[test]
fn use_calls_alias_and_missing_file_fails() {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dake_lang_use_{id}"));
    std::fs::create_dir_all(dir.join("rules")).unwrap();
    std::fs::write(dir.join("rules/greet.dake"), "action hi():\n    set(result, \"ok\")\n    print(\"hi\")\n").unwrap();
    let main = dir.join("main.dake");
    std::fs::write(&main, "use \"rules/greet.dake\" as tools::pack\nset(out, tools::pack.hi())\nprint(${out})\n").unwrap();
    let image = resolve_file(&main).unwrap();
    let program = lower(&image).unwrap();
    let mut executor = Executor::new();
    executor.execute_ir(&program).unwrap();
    assert!(executor.output().iter().any(|l| l == "ok"));
    let missing = write_script("missing", "use \"no-such.dake\" as csv\n");
    let err = resolve_file(&missing).unwrap_err();
    assert!(err.contains("找不到"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn path_rejects_dot_segments() {
    assert_err("files.read.str(\"foo/./a.txt\")\n", "路径不安全");
    assert_err("files.read.str(\"a/../b.txt\")\n", "路径不安全");
    let dir = std::env::temp_dir().join(format!("dake_path_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("note.txt");
    std::fs::write(&file, "ok").unwrap();
    let src = format!("print(files.read.str(\"./{}\"))\n", file.display());
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ok"), "{output:?}");
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink("note.txt", &link).unwrap();
    let via = format!("print(files.read.str(\"{}\"))\n", link.display());
    let output = assert_ok(&via);
    assert!(output.iter().any(|line| line == "ok"), "{output:?}");
    let outside = dir.join("out.txt");
    std::os::unix::fs::symlink("/etc/hostname", &outside).unwrap();
    let bad = format!("files.read.str(\"{}\")\n", outside.display());
    assert_err(&bad, "路径不安全");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn host_snapshot_returns_objects() {
    let output = assert_ok(
        "set(h, sys.host())\nprint(${h}.os)\nset(c, sys.cpu())\nprint(str(${c}.logical))\nset(m, sys.mem())\nif ${m}.used > ${m}.total:\n    print(\"bad-mem\")\nelse:\n    print(\"mem-ok\")\nset(d, sys.disk(\"/\"))\nprint(${d}.mount)\nif ${d}.free < 0:\n    print(\"bad-disk\")\nelse:\n    print(\"disk-ok\")\nset(all, sys.disks())\nprint(str(list.len(${all})))\n",
    );
    assert!(output.iter().any(|line| !line.is_empty() && line != "mem-ok" && line != "disk-ok" && line.parse::<i64>().is_err()), "{output:?}");
    assert!(output.iter().any(|line| line.parse::<i64>().ok().is_some_and(|n| n > 0)), "{output:?}");
    assert!(output.iter().any(|line| line == "mem-ok"), "{output:?}");
    assert!(output.iter().any(|line| line == "/"), "{output:?}");
    assert!(output.iter().any(|line| line == "disk-ok"), "{output:?}");
    assert_err("sys.disk(\"/no/such/dake/path\")\n", "3002");
}

#[test]
fn await_rejects_shared_write_and_merges_distinct_writes() {
    let shared = "action left():\n    set(same, 1)\naction right():\n    set(same, 2)\nawait left, right\n";
    let err = compile(shared).unwrap_err();
    assert!(err.contains("same"), "{err}");
    let output = assert_ok("action left():\n    set(a, \"L\")\naction right():\n    set(b, \"R\")\nawait left, right\nprint(${a})\nprint(${b})\n");
    assert!(output.iter().any(|l| l == "L"), "{output:?}");
    assert!(output.iter().any(|l| l == "R"), "{output:?}");
}

#[test]
fn await_branches_read_files_and_call_actions() {
    let dir = std::env::temp_dir().join(format!("dake_await_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let left = dir.join("left.txt");
    let right = dir.join("right.txt");
    std::fs::write(&left, "甲\n乙\n").unwrap();
    std::fs::write(&right, "丙\n").unwrap();
    let src = format!(
        "struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\naction title_of(path):\n    set(row, files.read(${{path}}, note))\n    set(result, ${{row}}.title)\naction left():\n    set(a, title_of(\"{left}\"))\naction right():\n    set(b, files.read.str(\"{right}\"))\nawait left, right\nprint(${{a}})\nprint(${{b}})\n",
        left = left.display(),
        right = right.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "甲"), "{output:?}");
    assert!(output.iter().any(|l| l.contains('丙')), "{output:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn catch_binds_error_text_before_body() {
    let output = assert_ok("error data_error:\n    \"数据处理错误\"\ncatch data_error as e:\n    print(${e})\n");
    assert!(output.iter().any(|l| l.contains("数据处理错误")), "{output:?}");
    assert_err("catch missing as e:\n    print(${e})\n", "未定义的错误");
}

#[test]
fn builtins_store_results_in_set_names() {
    let dir = std::env::temp_dir().join(format!("dake_lang_log_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = format!(
        "set(re, data.re(\"\\\\d+\"))\nset(ok, data.vali(\"email\", \"mail\", \"a@b.co\"))\nset(js, data.seria(\"json\", \"hi\"))\nset(packed, data.comp(\"hello-dake\"))\nset(plain, data.decomp(${{packed}}))\nset(sh, cmd(\"echo\", \"ping\"))\nlog.init(\"{}\", false)\nlog.info(\"noted\")\nprint(${{re}})\nprint(${{js}})\nprint(base64.encode(${{plain}}))\nprint(${{sh}})\n",
        dir.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l.contains("\\d+") || l.contains("d+")), "{output:?}");
    assert!(output.iter().any(|l| l.contains("hi")), "{output:?}");
    assert!(output.iter().any(|l| l == "aGVsbG8tZGFrZQ=="), "{output:?}");
    assert!(output.iter().any(|l| l.contains("ping")), "{output:?}");
    let log = std::fs::read_to_string(dir.join("dake.log")).unwrap();
    assert!(log.contains("noted"));
    assert!(!output.iter().any(|l| l.starts_with("_serialized") || l.starts_with("_compressed")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bytes_text_and_struct_codec() {
    let dir = std::env::temp_dir().join(format!("dake_bytes_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let note = dir.join("note.txt");
    let src = format!(
        "struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nstruct blob:\n    from: bytes\n    layout: whole\n    payload: bytes\nfiles.write.str(\"{p}\", \"甲\\n乙\")\nset(raw, files.read.bytes(\"{p}\"))\nset(text, base64.encode(${{raw}}))\nset(back, base64.decode(${{text}}))\nfiles.write.bytes(\"{p}\", ${{back}})\nset(doc, files.read(\"{p}\", note))\nprint(${{doc}}.title)\nprint(${{doc}}.body)\nset(doc, update(${{doc}}, title, \"丙\"))\nfiles.write(\"{p}\", ${{doc}})\nset(last, files.read.str(\"{p}\"))\nprint(${{last}})\nset(parts, text.lines(\"a\\nb\"))\nset(joined, text.join(${{parts}}, \"-\"))\nprint(${{joined}})\n",
        p = note.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "甲"), "{output:?}");
    assert!(output.iter().any(|l| l == "乙"), "{output:?}");
    assert!(output.iter().any(|l| l == "丙\n乙"), "{output:?}");
    assert!(output.iter().any(|l| l == "a-b"), "{output:?}");
    let bad = dir.join("bad.bin");
    std::fs::write(&bad, [0xff, 0xfe]).unwrap();
    assert_err(&format!("set(t, files.read.str(\"{}\"))\n", bad.display()), "UTF-8");
    assert_err(&format!("set(t, files.read.bytes(\"{p}\"))\nprint(${{t}})\n", p = note.display()), "base64.encode");
    assert_err(&format!("set(t, files.read.bytes(\"{p}\"))\nprint(str(${{t}}))\n", p = note.display()), "base64.encode");
    assert_err(
        &format!("set(raw, files.read.bytes(\"{p}\"))\nfiles.write.str(\"{out}\", ${{raw}})\n", p = note.display(), out = dir.join("x").display()),
        "需要字符串",
    );
    assert_err("set(t, files.read(\"nope.txt\", missing))\n", "找不到结构");
    assert_err("set(t, base64.decode(\"!!!!\"))\n", "非法");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn record_layouts_imports_and_rejections() {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dake_record_{id}"));
    std::fs::create_dir_all(dir.join("rules")).unwrap();
    let bin = dir.join("blob.bin");
    std::fs::write(dir.join("rules/csv.dake"), "struct note:\n    from: bytes\n    layout: whole\n    payload: bytes\n").unwrap();
    let main = dir.join("main.dake");
    let src = format!(
        "use \"rules/csv.dake\" as rules::csv\nfiles.write.bytes(\"{p}\", base64.decode(\"YWI=\"))\nset(doc, files.read(\"{p}\", rules::csv.note))\nset(doc, update(${{doc}}, payload, base64.decode(\"Y2Q=\")))\nfiles.write(\"{p}\", ${{doc}})\nset(back, files.read.bytes(\"{p}\"))\nprint(base64.encode(${{back}}))\n",
        p = bin.display()
    );
    std::fs::write(&main, src).unwrap();
    let program = lower(&resolve_file(&main).unwrap()).unwrap();
    let mut executor = Executor::new();
    executor.execute_ir(&program).unwrap();
    assert!(executor.output().iter().any(|l| l == "Y2Q="), "{:?}", executor.output());

    let one = dir.join("one.txt");
    let lines = format!(
        "struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nfiles.write.str(\"{p}\", \"仅标题\")\nset(doc, files.read(\"{p}\", note))\nprint(${{doc}}.title)\nprint(${{doc}}.body)\n",
        p = one.display()
    );
    let output = assert_ok(&lines);
    assert!(output.iter().any(|l| l == "仅标题"), "{output:?}");
    assert!(output.iter().any(|l| l.is_empty()), "{output:?}");

    let empty = dir.join("empty.txt");
    std::fs::write(&empty, "").unwrap();
    assert_err(
        &format!("struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nset(doc, files.read(\"{}\", note))\n", empty.display()),
        "至少需要一行",
    );
    assert_err("struct bad:\n    from: bytes\n    layout: lines\n    a: str\n    b: str\n", "只适用于");
    assert_err("struct bad:\n    from: str\n    layout: whole\n    a: str\n    b: str\n", "只能有一个字段");
    assert_err("struct bad:\n    layout: whole\n    a: str\n", "缺少 from");
    assert_err(
        &format!("struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nfiles.write.str(\"{p}\", \"甲\\n乙\")\nset(doc, files.read(\"{p}\", note))\nset(doc, update(${{doc}}, missing, \"x\"))\n", p = one.display()),
        "没有字段",
    );
    assert_err(
        &format!("struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nfiles.write.str(\"{p}\", \"甲\\n乙\")\nset(doc, files.read(\"{p}\", note))\nset(doc, update(${{doc}}, title, 1))\n", p = one.display()),
        "类型不符",
    );
    assert_err(
        &format!("files.write(\"{}\", \"文本\")\n", one.display()),
        "需要记录",
    );
    assert_err("files.read.str(\"../secret.txt\")\n", "路径不安全");
    assert_err("set(t, base64.encode(\"hi\"))\n", "只接受字节");
    assert_err(
        &format!("set(t, files.read.bytes(\"{}\"))\nprint(num(${{t}}))\n", one.display()),
        "不能转成数字",
    );
    assert_err("set(s, \"hi\")\nprint(${s}.title)\n", "需要记录");
    assert_err("set(parts, text.join(list.of(1), \"-\"))\n", "需要字符串");
    assert_err("struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\ndoing note\n", "未找到行为");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn data_ops_use_text_and_bytes() {
    let dir = std::env::temp_dir().join(format!("dake_data_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let note = dir.join("note.txt");
    let out = dir.join("plain.txt");
    let src = format!(
        "struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nfiles.write.str(\"{p}\", \"a@b.co\\nrest\")\nset(doc, files.read(\"{p}\", note))\nset(ok, data.vali(\"email\", ${{doc}}.title))\nprint(${{doc}}.title)\nset(hit, data.re.find(\"\\\\d+\", \"id 42\"))\nprint(${{hit}})\nset(nums, data.re.all(\"\\\\d\", \"a1b2\"))\nprint(text.join(${{nums}}, \"\"))\nset(n, data.deseria(\"json\", \"[1, 2]\"))\nprint(str(${{n}}))\nset(bin, data.seria(\"bin\", \"hi\"))\nset(back, data.deseria(\"bin\", ${{bin}}))\nprint(${{back}})\nset(packed, data.comp(\"hello-dake\"))\nset(plain, data.decomp(${{packed}}))\nfiles.write.bytes(\"{out}\", ${{plain}})\nprint(files.read.str(\"{out}\"))\n",
        p = note.display(),
        out = out.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "a@b.co"), "{output:?}");
    assert!(output.iter().any(|l| l == "42"), "{output:?}");
    assert!(output.iter().any(|l| l == "12"), "{output:?}");
    assert!(output.iter().any(|l| l == "[1, 2]"), "{output:?}");
    assert!(output.iter().any(|l| l == "hi"), "{output:?}");
    assert!(output.iter().any(|l| l == "hello-dake"), "{output:?}");
    assert_err("set(t, data.re.find(\"\\\\d+\", \"none\"))\n", "没有匹配");
    assert_err(
        &format!("struct note:\n    from: str\n    layout: lines\n    title: str\n    body: str\nfiles.write.str(\"{p}\", \"nope\\nrest\")\nset(doc, files.read(\"{p}\", note))\nset(ok, data.vali(\"email\", ${{doc}}.title))\n", p = note.display()),
        "邮箱",
    );
    assert_err("set(t, data.deseria(\"json\", \"null\"))\n", "空值");
    assert_err("set(t, data.decomp(\"hello\"))\n", "需要字节");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_each_streams_rows_without_a_list() {
    let dir = std::env::temp_dir().join(format!("dake_each_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("people.csv");
    std::fs::write(&path, "name,age\nann,1\nbee,2\n").unwrap();
    let src = format!(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\naction see(row):\n    print(${{row}}.name)\nset(n, files.each(\"{p}\", person, see, header: true))\nprint(str(${{n}}))\n",
        p = path.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    assert!(output.iter().any(|line| line == "bee"), "{output:?}");
    assert!(output.iter().any(|line| line == "2"), "{output:?}");
    assert!(!output.iter().any(|line| line == "name"), "{output:?}");
    std::fs::write(&path, "").unwrap();
    let empty = format!(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\naction see(row):\n    print(${{row}}.name)\nset(n, files.each(\"{p}\", person, see))\nprint(str(${{n}}))\n",
        p = path.display()
    );
    let output = assert_ok(&empty);
    assert!(output.iter().any(|line| line == "0"), "{output:?}");
    std::fs::write(&path, "ann,1\nnope\n").unwrap();
    let bad = format!(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\naction see(row):\n    print(${{row}}.name)\nfiles.each(\"{p}\", person, see)\n",
        p = path.display()
    );
    assert_err(&bad, "字段个数不符");
    let out = dir.join("out.csv");
    let write = format!(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\nset(a, record(person, name, \"ann\", age, 1))\nfiles.write.row(\"{p}\", ${{a}}, header: true)\nset(b, record(person, name, \"bee\", age, 2))\nfiles.write.row(\"{p}\", ${{b}}, header: true)\nprint(files.read.str(\"{p}\"))\n",
        p = out.display()
    );
    let output = assert_ok(&write);
    assert!(output.iter().any(|line| line.contains("name,age") && line.contains("ann,1") && line.contains("bee,2")), "{output:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn split_rows_and_list_each() {
    let dir = std::env::temp_dir().join(format!("dake_split_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("person.txt");
    let src = format!(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\n    ok: bool\nset(row, text.decode(\"ann,3,true\", person))\nprint(${{row}}.name)\nprint(str(${{row}}.age))\nprint(str(${{row}}.ok))\nfiles.write(\"{p}\", ${{row}})\nprint(files.read.str(\"{p}\"))\nset(rows, list.of())\neach ${{line}} in text.lines(\"ann,1,true\\nbee,2,false\"):\n    set(row, text.decode(${{line}}, person))\n    set(row, update(${{row}}, name, \"新\"))\n    set(rows, list.add(${{rows}}, text.encode(${{row}})))\nprint(text.join(${{rows}}, \"|\"))\neach ${{file}} in files:\n    print(${{file}})\n",
        p = path.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "ann"), "{output:?}");
    assert!(output.iter().any(|l| l == "3"), "{output:?}");
    assert!(output.iter().any(|l| l == "true"), "{output:?}");
    assert!(output.iter().any(|l| l == "ann,3,true"), "{output:?}");
    assert!(output.iter().any(|l| l == "新,1,true|新,2,false"), "{output:?}");
    assert!(!output.iter().any(|l| l.contains("person.txt")), "{output:?}");
    let shape = "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\n    ok: bool\n";
    assert_err(&format!("{shape}set(t, text.decode(\"a,b\", person))\n"), "字段个数不符");
    assert_err(&format!("{shape}set(t, text.decode(\"a,x,true\", person))\n"), "不是整数");
    assert_err(&format!("{shape}set(t, text.decode(\"a,1,yes\", person))\n"), "不是布尔值");
    assert_err("struct bad:\n    from: str\n    layout: lines\n    sep: \",\"\n    title: str\n    body: str\n", "只有 split");
    assert_err("set(rows, list.of())\neach ${item} in \"hi\":\n    print(${item})\n", "必须是列表");
}

#[test]
fn index_len_and_record() {
    let output = assert_ok(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\nset(rows, [\"ann\", \"bee\"])\nprint(${rows}[0])\nprint(str(list.len(${rows})))\nprint(str([0]))\nset(people, list.of())\nset(people, list.add(${people}, record(person, age, 3, name, \"ann\")))\nprint(${people}[0].name)\nprint(text.encode(${people}[0]))\n",
    );
    assert!(output.iter().any(|l| l == "ann"), "{output:?}");
    assert!(output.iter().any(|l| l == "2"), "{output:?}");
    assert!(output.iter().any(|l| l == "[0]"), "{output:?}");
    assert!(output.iter().any(|l| l == "ann,3"), "{output:?}");
    assert_err("set(rows, [\"ann\"])\nprint(${rows}[1])\n", "越界");
    assert_err("set(rows, [\"ann\"])\nprint(${rows}[-1])\n", "越界");
    assert_err("set(rows, \"ann\")\nprint(${rows}[0])\n", "只能用于列表");
    assert_err(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\nset(row, record(person, name, \"ann\"))\n",
        "必须正好是",
    );
    assert_err(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\nset(row, record(person, name, 1, age, 3))\n",
        "类型不符",
    );
}

#[test]
fn version_051_gaps() {
    let output = assert_ok(
        r#"struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
set(row, text.decode("\"a,b\",c", person))
print(${row}.name)
print(text.encode(${row}))
struct child:
    from: str
    layout: json
    name: str
struct parent:
    from: str
    layout: json
    who: child
    tags: list str
set(doc, text.decode("{\"who\":{\"name\":\"ann\"},\"tags\":[\"a\"]}", parent))
print(${doc}.who.name)
print(${doc}.tags[0])
set(who, record(child, name, "bee"))
set(doc, record(parent, tags, ["z"], who, ${who}))
print(${doc}.who.name)
set(g, data.re.group("(\\d+)", "id 42", 1))
print(${g})
set(rep, data.re.replace("\\d+", "a1b2", "x"))
print(${rep})
set(raw, utf8.encode("甲"))
print(utf8.decode(${raw}))
action greet(name):
    print(${name})
pipe run:
    greet("piped")
doing run
error data_error:
    "数据处理错误"
catch data_error as e:
    files.read.str("no-such-dake-051")
print(${e})
"#,
    );
    assert!(output.iter().any(|l| l == "a,b"), "{output:?}");
    assert!(output.iter().any(|l| l.contains("\"a,b\"")), "{output:?}");
    assert!(output.iter().any(|l| l == "ann"), "{output:?}");
    assert!(output.iter().any(|l| l == "bee"), "{output:?}");
    assert!(output.iter().any(|l| l == "42"), "{output:?}");
    assert!(output.iter().any(|l| l == "axbx"), "{output:?}");
    assert!(output.iter().any(|l| l == "甲"), "{output:?}");
    assert!(output.iter().any(|l| l == "piped"), "{output:?}");
    assert!(output.iter().any(|l| l.contains("no-such-dake-051")), "{output:?}");
}

#[test]
fn width_layout_roundtrip() {
    let dir = std::env::temp_dir().join(format!("dake_width_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("head.bin");
    let src = format!(
        r#"struct head:
    from: bytes
    layout: width
    magic: bytes 2
    count: int 2
    flag: bool 1
set(doc, record(head, magic, base64.decode("QUI="), count, 256, flag, true))
files.write("{p}", ${{doc}})
set(back, files.read("{p}", head))
print(base64.encode(${{back}}.magic))
print(str(${{back}}.count))
print(str(${{back}}.flag))
"#,
        p = path.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "QUI="), "{output:?}");
    assert!(output.iter().any(|l| l == "256"), "{output:?}");
    assert!(output.iter().any(|l| l == "true"), "{output:?}");
    let raw = std::fs::read(&path).unwrap();
    assert_eq!(raw, vec![b'A', b'B', 1, 0, 1]);
    assert_err(
        "struct head:\n    from: bytes\n    layout: width\n    count: int 3\n",
        "只能是 1、2、4 或 8",
    );
    assert_err(
        "struct head:\n    from: str\n    layout: lines\n    title: str 2\n    body: str\n",
        "只有 width",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn floats_catch_slice_and_stop() {
    let output = assert_ok(
        r#"set(n, 1.5 + 1)
print(str(${n}))
struct price:
    from: str
    layout: json
    amount: float
set(row, text.decode("{\"amount\":2.5}", price))
print(str(${row}.amount))
set(rows, ["a", "b", "c", "d"])
print(text.join(${rows}[1:3], ""))
each ${item} in ${rows}:
    if ${item} == "c":
        stop
    print(${item})
catch as e:
    files.read.str("no-such-dake-053")
else:
    print("failed")
print(${e})
"#,
    );
    assert!(output.iter().any(|l| l == "2.5"), "{output:?}");
    assert!(output.iter().any(|l| l == "bc"), "{output:?}");
    assert!(output.iter().any(|l| l == "a"), "{output:?}");
    assert!(output.iter().any(|l| l == "b"), "{output:?}");
    assert!(!output.iter().any(|l| l == "c"), "{output:?}");
    assert!(output.iter().any(|l| l == "failed"), "{output:?}");
    assert!(output.iter().any(|l| l.contains("no-such-dake-053")), "{output:?}");
    assert_err("stop\n", "不在 each");
}

#[test]
fn objects_lists_and_width_order() {
    let dir = std::env::temp_dir().join(format!("dake_054_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pack.bin");
    let src = format!(
        r#"struct kid:
    from: str
    layout: json
    name: str
struct parent:
    from: str
    layout: json
    kids: list kid
    one: kid
set(row, record(parent, kids, [record(kid, name, "ann")], one, record(kid, name, "bee")))
print(${{row}}.kids[0].name)
print(${{row}}.one.name)
set(obj, object("a", 1, "b", "x"))
print(str(${{obj}}.a))
print(${{obj}}["b"])
set(back, data.deseria("json", "{{\"a\":1}}"))
print(str(${{back}}.a))
set(rows, ["a", "b", "c"])
set(rows, list.set(${{rows}}, 1, "z"))
set(rows, list.remove(${{rows}}, 0))
print(text.join(${{rows}}, ""))
each ${{item}} at ${{i}} in ["a", "b"]:
    print(str(${{i}}))
    if ${{i}} == 1:
        stop
    print(${{item}})
struct pack:
    from: bytes
    layout: width
    order: le
    n: int 2
    amount: float 4
    rest: bytes
set(doc, record(pack, n, 256, amount, 1.5, rest, base64.decode("QQ==")))
files.write("{p}", ${{doc}})
set(doc, files.read("{p}", pack))
print(str(${{doc}}.n))
print(str(${{doc}}.amount))
print(base64.encode(${{doc}}.rest))
"#,
        p = path.display()
    );
    let output = assert_ok(&src);
    for line in ["ann", "bee", "1", "x", "zc", "0", "a"] {
        assert!(output.iter().any(|l| l == line), "{line} missing in {output:?}");
    }
    assert!(output.iter().any(|l| l == "1.5"), "{output:?}");
    assert!(output.iter().any(|l| l == "QQ=="), "{output:?}");
    assert!(!output.iter().any(|l| l == "b"), "{output:?}");
    let raw = std::fs::read(&path).unwrap();
    assert_eq!(raw, vec![0, 1, 0, 0, 0xc0, 0x3f, b'A']);
    assert_err("struct pack:\n    from: bytes\n    layout: width\n    amount: float 2\n", "只能是 4 或 8");
    let output = assert_ok(
        r#"action clean(text):
    set(result, ${text})
pipe seal(text):
    clean(${text})
set(obj, object("set", "ok"))
print(${obj}.set)
print(seal("piped"))
"#,
    );
    assert!(output.iter().any(|l| l == "ok"), "{output:?}");
    assert!(output.iter().any(|l| l == "piped"), "{output:?}");
    assert_err("list.add()\n", "需要 2 个参数");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rows_map_and_keep() {
    let dir = std::env::temp_dir().join(format!("dake_055_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("people.csv");
    std::fs::write(&path, "ann,a@b.co\nbee,b@c.co\n").unwrap();
    let src = format!(
        r#"struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
action rename(row):
    set(result, update(${{row}}, name, "新"))
action named(row):
    set(result, ${{row}}.name == "ann")
set(rows, files.rows("{p}", person))
set(rows, list.keep(${{rows}}, named))
set(rows, list.map(${{rows}}, rename))
files.write.rows("{p}", ${{rows}})
print(files.read.str("{p}"))
"#,
        p = path.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "新,a@b.co"), "{output:?}");
    assert_err("list.map([1], missing)\n", "找不到");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pack_records_into_manifest() {
    let dir = std::env::temp_dir().join(format!("dake_056_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let out = dir.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let src = format!(
        r#"lib:
    name: "people"
    version: "1.0.0"
    out_dir: "{out}"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
pack("people.csv", [record(person, name, "ann", email, "a@b.co")])
"#,
        out = out.display()
    );
    assert_ok(&src);
    let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    assert!(manifest.contains("people.csv"), "{manifest}");
    assert!(manifest.contains("person"), "{manifest}");
    let body = std::fs::read_to_string(out.join("people.csv")).unwrap();
    assert_eq!(body, "ann,a@b.co");
    assert_err("pack(\"a.csv\", [\"x\"])\n", "需要 lib");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unpack_uses_manifest_struct() {
    let dir = std::env::temp_dir().join(format!("dake_057_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let out = dir.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let packed = out.join("people.csv");
    let src = format!(
        r#"lib:
    name: "people"
    version: "1.0.0"
    out_dir: "{out}"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
pack("people.csv", [record(person, name, "ann", email, "a@b.co")])
set(rows, unpack("{file}"))
print(${{rows}}[0].name)
"#,
        out = out.display(),
        file = packed.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|l| l == "ann"), "{output:?}");
    let again = format!(
        r#"struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
set(rows, unpack("{file}"))
print(${{rows}}[0].email)
"#,
        file = packed.display()
    );
    let output = assert_ok(&again);
    assert!(output.iter().any(|l| l == "a@b.co"), "{output:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn decry_restores_packed_bytes() {
    let dir = std::env::temp_dir().join(format!("dake_decry_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let out = dir.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let src = format!(
        r#"lib:
    name: "people"
    version: "1.0.0"
    out_dir: "{out}"
files.encry()
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
pack("people.csv", [record(person, name, "ann", email, "a@b.co")])
"#,
        out = out.display()
    );
    assert_ok(&src);
    let enc = out.join("people.csv.enc");
    let key = out.join("dake.key");
    let again = format!(
        r#"set(plain, files.decry("{key}", files.read.bytes("{enc}")))
print(utf8.decode(${{plain}}))
"#,
        key = key.display(),
        enc = enc.display()
    );
    let output = assert_ok(&again);
    assert!(output.iter().any(|l| l == "ann,a@b.co"), "{output:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_carry_place_and_hint() {
    let missing = run("struct person:\n    layout: split\n    sep: \",\"\n    name: str\n");
    let text = missing.error.unwrap_or_default();
    assert!(text.contains("位置:"), "{text}");
    assert!(text.contains("建议:"), "{text}");
    assert!(text.contains("from"), "{text}");
    let runtime = run("files.read.str(\"no-such-dake-058\")\n");
    let text = runtime.error.unwrap_or_default();
    assert!(text.contains("位置:"), "{text}");
    assert!(text.contains("建议:"), "{text}");
}

#[test]
fn headers_list_and_pack_key() {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dake_rows_{id}"));
    let origin = std::env::current_dir().unwrap();
    let _guard = CWD.lock().unwrap();
    std::fs::create_dir_all(dir.join("data/sub")).unwrap();
    std::fs::create_dir_all(dir.join("data/skip")).unwrap();
    std::env::set_current_dir(&dir).unwrap();
    std::fs::write("people.csv", "name,email\nann,a@b.co\n").unwrap();
    std::fs::write("data/a.csv", "x\n").unwrap();
    std::fs::write("data/b.txt", "y\n").unwrap();
    std::fs::write("data/sub/c.csv", "z\n").unwrap();
    std::fs::write("data/skip/d.csv", "w\n").unwrap();
    let out = dir.join("out");
    let key = dir.join("pack.key");
    let src = format!(
        r#"
lib:
    name: "demo"
    version: "0.1.0"
    out_dir: "{out}"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
set(rows, files.rows("people.csv", person, header: true))
print(${{rows}}[0].name)
set(rows, list.map(${{rows}}, rename))
files.write.rows("people.csv", ${{rows}}, header: true)
action rename(row):
    set(result, update(${{row}}, name, "新"))
each ${{path}} in files.list("data", suffix: ".csv", deep: true, exclude: ["skip"]):
    print(${{path}})
pack("people.csv", ${{rows}}, key: "{key}")
files.encry()
"#,
        out = out.display(),
        key = key.display()
    );
    std::fs::write("people.csv", "email,name\nann,a@b.co\n").unwrap();
    let bad = run(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
set(rows, files.rows("people.csv", person, header: true))
"#,
    );
    let text = bad.error.unwrap_or_default();
    assert!(text.contains("表头"), "{text}");
    std::fs::write("people.csv", "name,email\nann,a@b.co\n").unwrap();
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    let written = std::fs::read_to_string("people.csv").unwrap();
    assert!(written.starts_with("name,email\n"), "{written}");
    assert!(output.iter().any(|line| line.ends_with("data/a.csv")), "{output:?}");
    assert!(output.iter().any(|line| line.ends_with("data/sub/c.csv")), "{output:?}");
    assert!(!output.iter().any(|line| line.ends_with("b.txt") || line.ends_with("d.csv")), "{output:?}");
    let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    assert!(manifest.contains("\"encrypted\": true"), "{manifest}");
    assert!(!manifest.contains("a.csv"), "{manifest}");
    assert!(out.join("people.csv").exists());
    assert!(!out.join("people.csv.enc").exists());
    let again = format!(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
set(rows, unpack("{csv}", key: "{key}"))
print(${{rows}}[0].name)
"#,
        csv = out.join("people.csv").display(),
        key = key.display()
    );
    let output = assert_ok(&again);
    assert!(output.iter().any(|line| line == "新"), "{output:?}");
    let missing = format!(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
unpack("{csv}")
"#,
        csv = out.join("people.csv").display()
    );
    let failed = run(&missing);
    let text = failed.error.unwrap_or_default();
    assert!(text.contains("补上 key"), "{text}");
    let _ = std::env::set_current_dir(&origin);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn list_update_and_join() {
    let src = r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
struct account:
    from: str
    layout: split
    sep: ","
    email: str
    plan: str
struct member:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
    plan: str
set(people, [record(person, name, "ann", email, "a@b.co")])
set(people, list.update(${people}, name, "新"))
print(${people}[0].name)
set(accounts, [record(account, email, "a@b.co", plan, "pro")])
set(joined, list.join(${people}, email, ${accounts}, email, member))
print(${joined}[0].plan)
"#;
    let output = assert_ok(src);
    assert!(output.iter().any(|line| line == "新"), "{output:?}");
    assert!(output.iter().any(|line| line == "pro"), "{output:?}");
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
struct note:
    from: str
    layout: split
    sep: ","
    title: str
list.update([record(person, name, "ann", email, "a@b.co"), record(note, title, "x")], name, "新")
"#,
        "同一种结构",
    );
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
struct account:
    from: str
    layout: split
    sep: ","
    email: str
    plan: str
struct member:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
    plan: str
    extra: str
list.join([record(person, name, "ann", email, "a@b.co")], email, [record(account, email, "a@b.co", plan, "pro")], email, member)
"#,
        "两边都没有",
    );
}

#[test]
fn shape_paths_and_append() {
    let src = r#"
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
set(rows, [record(person, name, "ann", email, "a@b.co", plan, "pro"), record(person, name, "bee", email, "b@c.co", plan, "free"), record(person, name, "cy", email, "c@d.co", plan, "pro")])
set(kept, list.where(${rows}, plan, "pro"))
print(list.len(${kept}))
set(picked, list.pick(${kept}, member))
print(${picked}[0].name)
set(sorted, list.sort(${rows}, name, order: "desc"))
each ${row} in ${sorted}:
    print(${row}.name)
set(grouped, list.group(${rows}, plan, by_plan))
print(${grouped}[0].plan)
print(${grouped}[0].people[0].name)
print(${grouped}[1].people[0].name)
print(files.name("data/a.csv"))
print(files.dir("data/a.csv"))
print(files.dir("a.csv"))
"#;
    let output = assert_ok(src);
    assert_eq!(
        output,
        vec![
            "2".to_string(),
            "ann".to_string(),
            "cy".to_string(),
            "bee".to_string(),
            "ann".to_string(),
            "pro".to_string(),
            "ann".to_string(),
            "bee".to_string(),
            "a.csv".to_string(),
            "data".to_string(),
            ".".to_string(),
        ]
    );
    let stable = assert_ok(r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
set(rows, [record(person, name, "ann", email, "a"), record(person, name, "bee", email, "c"), record(person, name, "ann", email, "b")])
set(rows, list.sort(${rows}, name))
each ${row} in ${rows}:
    print(${row}.email)
"#);
    assert_eq!(stable, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
struct member:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
    plan: str
list.pick([record(person, name, "ann", email, "a@b.co")], member)
"#,
        "来源没有字段",
    );
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    plan: str
struct bad:
    from: str
    layout: json
    plan: str
    name: str
list.group([record(person, name, "ann", plan, "pro")], plan, bad)
"#,
        "组键和一个列表",
    );
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dake_append_{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("people.csv");
    std::fs::write(&file, "ann,a@b.co").unwrap();
    let append = format!(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
files.write.rows("{file}", [record(person, name, "bee", email, "b@c.co")], append: true)
"#,
        file = file.display()
    );
    assert_ok(&append);
    let body = std::fs::read_to_string(&file).unwrap();
    assert_eq!(body, "ann,a@b.co\nbee,b@c.co");
    let both = format!(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str
files.write.rows("{file}", [record(person, name, "cy", email, "c@d.co")], append: true, header: true)
"#,
        file = file.display()
    );
    assert_err(&both, "append");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn struct_contract_checks_and_replaces() {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dake_contract_{id}"));
    let out = dir.join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("people.csv");
    std::fs::write(&csv, "ann,not-an-email\n").unwrap();
    let bad_rows = format!(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str check: not_empty
    email: str check: email
set(rows, files.rows("{csv}", person))
"#,
        csv = csv.display()
    );
    assert_err(&bad_rows, "建议:");
    assert_err(&bad_rows, "合法邮箱");
    let src = format!(
        r#"
lib:
    name: "people"
    version: "1.0.0"
    out_dir: "{out}"
struct person_v1:
    from: str
    layout: split
    sep: ","
    name: str
    mail: str
struct person:
    from: str
    layout: split
    sep: ","
    replaces: person_v1
    name: str check: not_empty
    email: str check: email take: mail
    plan: str default: "free"
set(row, record(person, name, "ann", email, "a@b.co"))
print(${{row}}.plan)
pack("people.csv", [record(person_v1, name, "ann", mail, "a@b.co")])
set(rows, unpack("people.csv"))
print(${{rows}}[0].email)
print(${{rows}}[0].plan)
"#,
        out = out.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "free"));
    assert!(output.iter().any(|line| line == "a@b.co"), "{output:?}");
    assert_err(
        r#"
lib:
    name: "people"
    version: "1.0.0"
    out_dir: "out"
struct person:
    from: str
    layout: split
    sep: ","
    name: str
    email: str check: email
pack("people.csv", [record(person, name, "ann", email, "bad")])
"#,
        "email",
    );
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    name: str check: not_empty
    email: str check: email
set(row, record(person, name, "ann", email, "a@b.co"))
set(row, update(${row}, email, "bad"))
"#,
        "email",
    );
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    email: str check: email default: ""
"#,
        "邮箱",
    );
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    count: int check: numeric
"#,
        "只有 str",
    );
    assert_err(
        r#"
struct person:
    from: str
    layout: split
    sep: ","
    count: int check: numeric
"#,
        "check 只写在 str",
    );
    assert_err(
        r#"
struct old:
    from: str
    layout: split
    sep: ","
    name: str
struct one:
    from: str
    layout: split
    sep: ","
    replaces: old
    name: str
struct two:
    from: str
    layout: split
    sep: ","
    replaces: old
    name: str
"#,
        "替换了两次",
    );
    assert_err(
        r#"
struct old:
    from: str
    layout: split
    sep: ","
    name: str
struct one:
    from: str
    layout: split
    sep: ","
    replaces: old
    name: str
struct two:
    from: str
    layout: split
    sep: ","
    replaces: old
    name: str
"#,
        "只保留一个 replaces",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn http_reply(body: &str, status: u16) -> Vec<u8> {
    format!("HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
}

fn read_http(stream: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
        if let Some(index) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..index]).to_string();
            let length = head.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok())?
            }).unwrap_or(0);
            if raw.len() - (index + 4) >= length {
                break;
            }
        }
    }
    Ok(String::from_utf8_lossy(&raw).to_string())
}

#[test]
fn net_get_and_post_use_url_target() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = std::sync::Arc::clone(&seen);
    let server = std::thread::spawn(move || {
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http(&mut stream).unwrap_or_default();
            record.lock().unwrap().push(request.clone());
            let status = if request.starts_with("GET /v1/missing ") { 404 } else { 200 };
            let body = if status == 200 { "{\"name\":\"ann\",\"email\":\"a@b.co\"}" } else { "" };
            let _ = stream.write_all(&http_reply(body, status));
        }
    });
    let base = format!("http://127.0.0.1:{port}/v1");
    let once = format!("http://127.0.0.1:{port}/once");
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
    email: str check: email
struct auth:
    from: str
    layout: json
    authorization: str
net.url("{base}", headers: record(auth, authorization, "Bearer t"))
set(row, net.get(person))
print(${{row}}.name)
set(row, net.get("/people/1", person))
net.post(${{row}})
set(row, net.get("{once}", person))
set(row, net.get(person))
print(${{row}}.name)
"#,
        base = base,
        once = once
    );
    let output = assert_ok(&src);
    assert!(output.iter().filter(|line| *line == "ann").count() >= 2, "{output:?}");
    let missing = format!(
        r#"
net.url("{base}")
net.get("/missing")
"#,
        base = base
    );
    let failed = run(&missing);
    let text = failed.error.unwrap_or_default();
    assert!(text.contains("4033"), "{text}");
    assert!(text.contains("建议:"), "{text}");
    server.join().unwrap();
    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|item| item.contains("GET /v1 HTTP/1.1") && item.contains("Authorization: Bearer t")), "{seen:?}");
    assert!(seen.iter().any(|item| item.contains("GET /v1/people/1 ")), "{seen:?}");
    assert!(seen.iter().any(|item| item.starts_with("POST /v1 ") && item.contains("Content-Type: application/json")), "{seen:?}");
    assert!(seen.iter().any(|item| item.contains("GET /once ")), "{seen:?}");
    let after_once = seen.iter().rposition(|item| item.contains("GET /once ")).unwrap();
    assert!(seen.iter().skip(after_once + 1).any(|item| item.contains("GET /v1 ")), "{seen:?}");
    assert_err("net.get()\n", "4030");
    assert_err("net.url(\"ftp://example\")\n", "4030");
    assert_err("net.url(\"https://example\", cert: \"a.pem\")\n", "4032");
}

#[test]
fn net_https_uses_ca_without_changing_get() {
    let key = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    let cert = params.self_signed(&key).unwrap();
    let dir = std::env::temp_dir().join(format!("dake_tls_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let ca = dir.join("ca.pem");
    std::fs::write(&ca, cert.pem()).unwrap();
    let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(cert.pem().into_bytes())).collect::<Result<Vec<_>, _>>().unwrap();
    let private = rustls_pemfile::private_key(&mut std::io::Cursor::new(key.serialize_pem().into_bytes())).unwrap().unwrap();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = std::sync::Arc::new(rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, private).unwrap());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut conn = rustls::ServerConnection::new(std::sync::Arc::clone(&config)).unwrap();
            let mut tls = rustls::Stream::new(&mut conn, &mut stream);
            if read_http(&mut tls).is_ok() {
                let _ = tls.write_all(&http_reply("{\"name\":\"ann\",\"email\":\"a@b.co\"}", 200));
            }
        }
    });
    let base = format!("https://localhost:{port}/v1");
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
    email: str
net.url("{base}", ca: "{ca}")
set(row, net.get(person))
print(${{row}}.name)
"#,
        base = base,
        ca = ca.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    let rejected = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
    email: str
net.url("{base}")
net.get(person)
"#,
        base = base
    );
    let failed = run(&rejected);
    let text = failed.error.unwrap_or_default();
    assert!(text.contains("4032"), "{text}");
    assert!(text.contains("建议:"), "{text}");
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn net_accept_one_request() {
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let address = format!("127.0.0.1:{port}");
    let client_address = address.clone();
    let client = std::thread::spawn(move || {
        let mut last = String::new();
        for _ in 0..50 {
            match std::net::TcpStream::connect(&client_address) {
                Ok(mut stream) => {
                    let body = "{\"name\":\"ann\"}";
                    let request = format!("POST / HTTP/1.1\r\nHost: {client_address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    stream.write_all(request.as_bytes()).unwrap();
                    return read_http(&mut stream).unwrap_or_default();
                }
                Err(err) => last = err.to_string(),
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("连不上 {client_address}: {last}");
    });
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
action answer(row, headers):
    print(${{headers}}["Content-Type"])
    set(status, 201)
    set(headers, object("Location", "/people/1"))
    set(result, update(${{row}}, name, "ok"))
set(reply, net.accept("{address}", person, answer))
print(${{reply}}.name)
"#
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ok"), "{output:?}");
    assert!(output.iter().any(|line| line == "application/json"), "{output:?}");
    let response = client.join().unwrap();
    assert!(response.contains("HTTP/1.1 201"), "{response}");
    assert!(response.contains("Location: /people/1"), "{response}");
    assert!(response.contains("\"name\":\"ok\""), "{response}");
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${row}.name)\nnet.accept(\"https://127.0.0.1:9\", person, answer)\n",
        "主机:端口",
    );
}

#[test]
fn net_accept_one_tls_request() {
    assert_err("struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${row})\nnet.accept(\"127.0.0.1:59999\", person, answer, cert: \"only.pem\")\n", "必须同时");
    let key = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    let cert = params.self_signed(&key).unwrap();
    let dir = std::env::temp_dir().join(format!("dake_accept_tls_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    std::fs::write(&cert_path, cert.pem()).unwrap();
    std::fs::write(&key_path, key.serialize_pem()).unwrap();
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let pem = cert.pem();
    let client = std::thread::spawn(move || {
        let mut roots = rustls::RootCertStore::empty();
        for item in rustls_pemfile::certs(&mut std::io::Cursor::new(pem.into_bytes())) {
            roots.add(item.unwrap()).unwrap();
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = std::sync::Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
        let address = format!("127.0.0.1:{port}");
        for _ in 0..50 {
            if let Ok(mut tcp) = std::net::TcpStream::connect(&address) {
                let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
                let mut session = rustls::ClientConnection::new(config, name).unwrap();
                let mut tls = rustls::Stream::new(&mut session, &mut tcp);
                let body = "{\"name\":\"ann\"}";
                let request = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                tls.write_all(request.as_bytes()).unwrap();
                return read_http(&mut tls).unwrap_or_default();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("等待接入");
    });
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
action answer(row):
    set(result, update(${{row}}, name, "ok"))
set(reply, net.accept("127.0.0.1:{port}", person, answer, cert: "{cert}", key: "{key}"))
print(${{reply}}.name)
"#,
        cert = cert_path.display(),
        key = key_path.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ok"), "{output:?}");
    let response = client.join().unwrap();
    assert!(response.contains("\"name\":\"ok\""), "{response}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_routes_and_preflight() {
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row, id):\n    set(result, ${row})\nurl api \"127.0.0.1:9\"\nserve:\n    route api \"/{a}\" person answer\n    route api \"/{b}\" person answer\n",
        "特异度",
    );
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let dir = std::env::temp_dir().join(format!("dake_serve_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "{\"name\":\"a\"}").unwrap();
    std::fs::write(dir.join("b.json"), "{\"name\":\"b\"}").unwrap();
    let inbox = dir.join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("a.txt"), "{\"name\":\"a\"}").unwrap();
    std::fs::write(inbox.join("b.json"), "{\"name\":\"b\"}").unwrap();
    let out = dir.join("out");
    let address = format!("127.0.0.1:{port}");
    let client_address = address.clone();
    let inbox_live = inbox.clone();
    let client = std::thread::spawn(move || {
        let mut last = String::new();
        for _ in 0..50 {
            if let Ok(mut stream) = std::net::TcpStream::connect(&client_address) {
                let body = "{\"name\":\"ann\"}";
                let request = format!("POST /people/ann HTTP/1.1\r\nHost: {client_address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(request.as_bytes()).unwrap();
                let first = read_http(&mut stream).unwrap_or_default();
                let mut stream = std::net::TcpStream::connect(&client_address).unwrap();
                stream.write_all(b"GET /missing HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                let missing = read_http(&mut stream).unwrap_or_default();
                let mut stream = std::net::TcpStream::connect(&client_address).unwrap();
                stream.write_all(b"GET /%ZZ HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                let bad = read_http(&mut stream).unwrap_or_default();
                let mut stream = std::net::TcpStream::connect(&client_address).unwrap();
                stream.write_all(b"POST /boom HTTP/1.1\r\nHost: x\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"name\":\"x\"}").unwrap();
                let boom = read_http(&mut stream).unwrap_or_default();
                std::fs::write(inbox_live.join("live.json"), "{\"name\":\"no\"}").unwrap();
                std::fs::write(inbox_live.join("live.json"), "{\"name\":\"yes\"}").unwrap();
                return (first, missing, bad, boom);
            }
            last = "等待服务".into();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("{last}");
    });
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
action answer(row, id):
    print(${{id}})
    set(result, ${{row}})
action boom(row):
    set(result, 1)
action named(row, name):
    print(${{name}})
    set(result, ${{row}})
action plain(row):
    print("plain")
    set(result, ${{row}})
action watch(row, name):
    print(${{row}}.name)
    set(result, ${{row}})
    if ${{row}}.name == "yes":
        stop
lib:
    name: "box"
    version: "1.0.0"
    out_dir: "{out}"
url api "{address}"
dir inbox "{inbox}"
serve:
    route api "/people/{{id}}" person answer
    route api "/boom" person boom
    route inbox "/{{name}}.json" person watch
    route inbox person plain
"#,
        out = out.display(),
        address = address,
        inbox = inbox.display()
    );
    let output = assert_ok(&src);
    let (ok, missing, bad, boom) = client.join().unwrap();
    assert!(ok.contains("HTTP/1.1 200"), "{ok}");
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    assert!(missing.contains("404"), "{missing}");
    assert!(bad.contains("400"), "{bad}");
    assert!(boom.contains("500"), "{boom}");
    assert!(output.iter().any(|line| line == "plain"), "{output:?}");
    assert!(output.iter().any(|line| line == "b"), "{output:?}");
    assert!(output.iter().any(|line| line == "yes"), "{output:?}");
    assert!(!output.iter().any(|line| line == "no"), "{output:?}");
    assert!(out.join("manifest.json").is_file(), "包没有写出");
    let image = resolve_file(&write_script("pre", "files.read.str(\"/no/such/dake-file\")\n")).unwrap();
    let failed = crate::executor::preflight::check_image(&image).unwrap_err();
    assert!(failed.contains("4037"), "{failed}");
    let image = resolve_file(&write_script("pre", "if false:\n    files.read.str(\"/no/such/dake-file\")\n")).unwrap();
    assert!(crate::executor::preflight::check_image(&image).is_ok());
    let parent = dir.join("later");
    std::fs::create_dir_all(&parent).unwrap();
    let later = parent.join("row.txt");
    let src = format!("files.write.str(\"{}\", \"hi\")\nfiles.read.str(\"{}\")\n", later.display(), later.display());
    let image = resolve_file(&write_script("pre", &src)).unwrap();
    assert!(crate::executor::preflight::check_image(&image).is_ok(), "先写后读不应算缺失");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_specificity_filters_and_preflight_edges() {
    assert_err("route api \"/x\" person answer\n", "serve 外面");
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${row})\nurl api \"127.0.0.1:9\"\nserve:\n    route api \"/**\" person answer\nprint(\"after\")\n",
        "后面",
    );
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${row})\nurl api \"127.0.0.1:9\"\nurl api \"127.0.0.1:8\"\nserve:\n    route api \"/x\" person answer\n",
        "重复",
    );
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${row})\nurl api \"127.0.0.1:9\"\nserve:\n    route api \"/**/x\" person answer\n",
        "**",
    );
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = held.local_addr().unwrap().port();
    assert_err(
        &format!("struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${{row}})\nurl api \"127.0.0.1:{busy}\"\nserve:\n    route api \"/x\" person answer\n"),
        "4035",
    );
    drop(held);
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${row})\ndir missing \"/no/such/dake-serve-dir\"\nserve:\n    route missing person answer\n",
        "4035",
    );
    let root = std::env::temp_dir().join(format!("dake_edge_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&root);
    let inbox = root.join("inbox");
    std::fs::create_dir_all(inbox.join("sub")).unwrap();
    std::fs::create_dir_all(inbox.join("skip")).unwrap();
    std::fs::write(inbox.join("sub/ok.json"), "{\"name\":\"ok\"}").unwrap();
    std::fs::write(inbox.join("skip/no.json"), "{\"name\":\"no\"}").unwrap();
    let locked = root.join("locked");
    let readonly = root.join("readonly");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::create_dir_all(&readonly).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o555)).unwrap();
    let list_src = format!("files.list(\"{}\")\n", locked.display());
    let list_image = resolve_file(&write_script("pre", &list_src)).unwrap();
    let list_err = crate::executor::preflight::check_image(&list_image);
    let write_src = format!("files.write.str(\"{}/x.txt\", \"a\")\n", readonly.display());
    let write_image = resolve_file(&write_script("pre", &write_src)).unwrap();
    let write_err = crate::executor::preflight::check_image(&write_image);
    let missing = resolve_file(&write_script("pre", "dir inbox \"/no/such/dake-preflight-dir\"\n")).unwrap();
    let missing_err = crate::executor::preflight::check_image(&missing).unwrap_err();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o755)).unwrap();
    if unsafe { libc::geteuid() } != 0 {
        let list_text = list_err.expect_err("不能列出的目录应失败");
        let write_text = write_err.expect_err("不能写入的目录应失败");
        assert!(list_text.contains("4037"), "{list_text}");
        assert!(write_text.contains("4037"), "{write_text}");
    }
    assert!(missing_err.contains("4037"), "{missing_err}");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let address = format!("127.0.0.1:{port}");
    let client_address = address.clone();
    let client = std::thread::spawn(move || {
        fn once(address: &str, path: &str) -> String {
            let mut last = String::new();
            for _ in 0..50 {
                if let Ok(mut stream) = std::net::TcpStream::connect(address) {
                    let body = "{\"name\":\"ann\"}";
                    let request = format!("POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    stream.write_all(request.as_bytes()).unwrap();
                    return read_http(&mut stream).unwrap_or_default();
                }
                last = "等待".into();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!("{last}");
        }
        let lit = once(&client_address, "/a/b");
        let param = once(&client_address, "/a/c");
        let rest = once(&client_address, "/z/q");
        let done = once(&client_address, "/done");
        (lit, param, rest, done)
    });
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
action lit(row):
    print("lit")
    set(result, ${{row}})
action param(row, id):
    print("param")
    set(result, ${{row}})
action star(row):
    print("star")
    set(result, ${{row}})
action rest(row, rest):
    print(${{rest}})
    set(result, ${{row}})
    if ${{rest}} == "done":
        stop
action file(row, path):
    print(${{row}}.name)
    set(result, ${{row}})
url api "{address}"
dir box "{inbox}" deep: true exclude: ["skip"]
serve:
    route api "/a/b" person lit
    route api "/a/{{id}}" person param
    route api "/a/*" person star
    route api "/{{**rest}}" person rest
    route box "/{{**path}}" person file
"#,
        address = address,
        inbox = inbox.display()
    );
    let output = assert_ok(&src);
    let _ = client.join().unwrap();
    assert!(output.iter().any(|line| line == "lit"), "{output:?}");
    assert!(output.iter().any(|line| line == "param"), "{output:?}");
    assert!(!output.iter().any(|line| line == "star"), "{output:?}");
    assert!(output.iter().any(|line| line == "z/q"), "{output:?}");
    assert!(output.iter().any(|line| line == "ok"), "{output:?}");
    assert!(!output.iter().any(|line| line == "no"), "{output:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn serve_url_listens_with_tls() {
    assert_err("url api \"127.0.0.1:9\" cert: \"only.pem\"\nserve:\n    route api \"/\" person answer\n", "必须同时");
    let key = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    let cert = params.self_signed(&key).unwrap();
    let dir = std::env::temp_dir().join(format!("dake_serve_tls_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    std::fs::write(&cert_path, cert.pem()).unwrap();
    std::fs::write(&key_path, key.serialize_pem()).unwrap();
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let pem = cert.pem();
    let client = std::thread::spawn(move || {
        let mut roots = rustls::RootCertStore::empty();
        for item in rustls_pemfile::certs(&mut std::io::Cursor::new(pem.into_bytes())) {
            roots.add(item.unwrap()).unwrap();
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = std::sync::Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
        let address = format!("127.0.0.1:{port}");
        for _ in 0..50 {
            if let Ok(mut tcp) = std::net::TcpStream::connect(&address) {
                let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
                let mut session = rustls::ClientConnection::new(config, name).unwrap();
                let mut tls = rustls::Stream::new(&mut session, &mut tcp);
                let body = "{\"name\":\"ann\"}";
                let request = format!("POST /person HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                tls.write_all(request.as_bytes()).unwrap();
                return read_http(&mut tls).unwrap_or_default();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("等待服务");
    });
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
action answer(row):
    print(${{row}}.name)
    set(result, ${{row}})
    stop
url api "127.0.0.1:{port}" cert: "{cert}" key: "{key}"
serve:
    route api "/person" person answer
"#,
        cert = cert_path.display(),
        key = key_path.display()
    );
    let output = assert_ok(&src);
    let body = client.join().unwrap();
    assert!(body.contains("200"), "{body}");
    assert!(body.contains("ann"), "{body}");
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_workers_handle_two_connections() {
    assert_err("serve workers: 0:\n    route api \"/\" person answer\n", "1 到 64");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let address = format!("127.0.0.1:{port}");
    let client_address = address.clone();
    let client = std::thread::spawn(move || {
        for _ in 0..50 {
            if let (Ok(mut first), Ok(mut second)) = (std::net::TcpStream::connect(&client_address), std::net::TcpStream::connect(&client_address)) {
                let body = "{\"name\":\"ann\"}";
                let request = format!("POST /person HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                first.write_all(request.as_bytes()).unwrap();
                second.write_all(request.replace("ann", "bee").as_bytes()).unwrap();
                let a = read_http(&mut first).unwrap_or_default();
                let b = read_http(&mut second).unwrap_or_default();
                if let Ok(mut done) = std::net::TcpStream::connect(&client_address) {
                    let bye = "{\"name\":\"end\"}";
                    let request = format!("POST /done HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bye}", bye.len());
                    let _ = done.write_all(request.as_bytes());
                    let _ = read_http(&mut done);
                }
                return (a, b);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("等待服务");
    });
    let src = format!(
        r#"
struct person:
    from: str
    layout: json
    name: str
action answer(row):
    print(${{row}}.name)
    print(str(shared.add(n, 1)))
    set(result, ${{row}})
action quit(row):
    set(result, ${{row}})
    stop
set(n, 0)
share n
url api "{address}"
serve workers: 2:
    route api "/person" person answer
    route api "/done" person quit
"#
    );
    let output = assert_ok(&src);
    let (a, b) = client.join().unwrap();
    assert!(a.contains("200") && b.contains("200"), "{a} {b}");
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    assert!(output.iter().any(|line| line == "bee"), "{output:?}");
    assert!(output.iter().any(|line| line == "1"), "{output:?}");
    assert!(output.iter().any(|line| line == "2"), "{output:?}");
}

#[test]
fn write_rows_append_does_not_reread_the_file() {
    let dir = std::env::temp_dir().join(format!("dake_append_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("people.csv");
    std::fs::write(&path, "ann,1").unwrap();
    let src = format!(
        "struct person:\n    from: str\n    layout: split\n    sep: \",\"\n    name: str\n    age: int\nset(rows, list.of(record(person, name, \"bee\", age, 2)))\nfiles.write.rows(\"{p}\", ${{rows}}, append: true)\nprint(files.read.str(\"{p}\"))\n",
        p = path.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line.contains("ann,1\nbee,2")), "{output:?}");
    let block = dir.join("note.bin");
    let src = format!(
        "struct note:\n    from: bytes\n    layout: block\n    name: bytes\n    body: bytes\nset(row, record(note, name, utf8.encode(\"ann\"), body, utf8.encode(\"hello\")))\nfiles.write(\"{p}\", ${{row}})\nset(back, files.read(\"{p}\", note))\nprint(utf8.decode(${{back}}.name))\nprint(utf8.decode(${{back}}.body))\naction take(name, value):\n    print(utf8.decode(${{value}}))\nfiles.field(\"{p}\", note, take)\n",
        p = block.display()
    );
    let output = assert_ok(&src);
    assert!(output.iter().any(|line| line == "ann"), "{output:?}");
    assert!(output.iter().any(|line| line == "hello"), "{output:?}");
    assert_err("struct note:\n    from: str\n    layout: block\n    name: bytes\n", "只能是 bytes");
    assert_err(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(m, 1)\n    set(result, ${row})\nset(n, 0)\nshare n\nurl api \"127.0.0.1:9\"\nserve workers: 2:\n    route api \"/\" person answer\n",
        "只能写 result",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repo_stores_package_and_serves_protocol() {
    assert_err("lib:\n    name: \"box\"\n", "缺少 version");
    assert_err("repo:\n    name: \"desk\"\n", "缺少 dir");
    let root = std::env::temp_dir().join(format!("dake_repo_{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&root);
    let src_dir = root.join("src");
    let out = root.join("out");
    let repo = root.join("desk");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(src_dir.join("a.txt"), b"hello").unwrap();
    let build = format!(
        "lib:\n    name: \"box\"\n    version: \"0.3.0\"\n    out_dir: \"{}\"\n    mods: [\"tools\"]\nrepo:\n    name: \"desk\"\n    dir: \"{}\"\nfiles.encry()\nfiles(\"{}/a.txt\")\n",
        out.display(),
        repo.display(),
        src_dir.display()
    );
    assert_ok(&build);
    let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    assert!(manifest.contains("\"mods\": [\"tools\"]"), "{manifest}");
    assert!(out.join("dake.key").is_file());
    let stored = repo.join("box").join("0.3.0");
    assert!(stored.join("a.txt.enc").is_file());
    assert!(!stored.join("dake.key").exists());
    assert_err(&format!("repo:\n    name: \"desk\"\n    dir: \"{}\"\nrepo.put(\"{}\")\n", repo.display(), out.display()), "已存在");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let fresh = root.join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    let manifest_bytes = std::fs::read(out.join("manifest.json")).unwrap();
    let file_bytes = std::fs::read(stored.join("a.txt.enc")).unwrap();
    let upload = file_bytes.clone();
    let client = std::thread::spawn(move || {
        let address = format!("127.0.0.1:{port}");
        for _ in 0..50 {
            if let Ok(mut stream) = std::net::TcpStream::connect(&address) {
                let manifest_res = http_put(&mut stream, "/dake/v1/box/0.3.0/manifest", &manifest_bytes);
                let mut file_stream = std::net::TcpStream::connect(&address).unwrap();
                let file_res = http_put(&mut file_stream, "/dake/v1/box/0.3.0/file/a.txt.enc", &upload);
                let mut post = std::net::TcpStream::connect(&address).unwrap();
                let post_res = http_put(&mut post, "/dake/v1/box/0.3.0", b"");
                let _ = std::net::TcpStream::connect(&address).and_then(|mut done| {
                    let body = b"{\"name\":\"x\"}";
                    let request = format!("POST /stop HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    done.write_all(request.as_bytes())?;
                    done.write_all(body)?;
                    read_http(&mut done)
                });
                assert!(manifest_res.contains("201"), "{manifest_res}");
                assert!(file_res.contains("201"), "{file_res}");
                assert!(post_res.contains("201"), "{post_res}");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("等待仓库");
    });
    let serve = format!(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction stop(row):\n    set(result, ${{row}})\n    stop\nrepo:\n    name: \"desk\"\n    dir: \"{}\"\nurl api \"127.0.0.1:{port}\"\nserve:\n    repo api\n    route api \"/stop\" person stop\n",
        fresh.display()
    );
    assert_ok(&serve);
    client.join().unwrap();
    assert_eq!(std::fs::read(fresh.join("box/0.3.0/a.txt.enc")).unwrap(), file_bytes);
    assert!(!fresh.join("box/0.3.0/dake.key").exists());
    let _ = std::fs::remove_dir_all(&root);
}

fn http_put(stream: &mut std::net::TcpStream, path: &str, body: &[u8]) -> String {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let method = if path.ends_with("/0.3.0") { "POST" } else { "PUT" };
    let request = format!("{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    read_http(stream).unwrap_or_else(|err| err.to_string())
}
