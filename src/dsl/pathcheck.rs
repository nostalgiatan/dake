/// 运行前和运行时共用的路径检查。
/// 去掉开头的 `./`，拒绝空路径、空字节、`..` 以及单独的 `.` 段。
pub fn tighten(path: &str) -> Result<String, String> {
    if path.is_empty() || path.contains('\0') || path.contains("..") {
        return Err(format!("路径不安全: {path}"));
    }
    let mut rest = path;
    while let Some(next) = rest.strip_prefix("./") {
        rest = next;
    }
    if rest.is_empty() || rest == "." {
        return Err(format!("路径不安全: {path}"));
    }
    for part in rest.split(['/', '\\']) {
        if part == "." || part == ".." {
            return Err(format!("路径不安全: {path}"));
        }
    }
    Ok(rest.to_string())
}
