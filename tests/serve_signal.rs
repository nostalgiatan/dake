use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn serve_stops_on_sigterm() {
    let dir = std::env::temp_dir().join(format!("dake_sig_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let script = dir.join("run.dake");
    let src = format!(
        "struct person:\n    from: str\n    layout: json\n    name: str\naction answer(row):\n    set(result, ${{row}})\nlib:\n    name: \"sig\"\n    version: \"0.0.1\"\n    out_dir: \"{}\"\nurl gate \"127.0.0.1:{port}\"\nserve:\n    route gate \"/person\" person answer\n",
        out.display()
    );
    std::fs::write(&script, src).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dake"))
        .args(["run", script.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut up = false;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            up = true;
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            let mut err = String::new();
            if let Some(stderr) = child.stderr.take() {
                use std::io::Read;
                let mut reader = stderr;
                let _ = reader.read_to_string(&mut err);
            }
            panic!("服务提前退出 {status}: {err}");
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(up, "服务没有开始监听");
    let _ = Command::new("kill").args(["-TERM", &child.id().to_string()]).status();
    let id = child.id();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        let _ = Command::new("kill").args(["-KILL", &id.to_string()]).status();
    });
    let finished = child.wait().unwrap();
    assert!(finished.success(), "{finished}");
    assert!(out.join("manifest.json").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}
