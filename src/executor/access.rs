use super::executor::ExecutionError;
use std::path::Path;

pub(super) fn probe_dir(path: &str) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|_| format!("目录不存在: {path}"))?;
    if !meta.is_dir() {
        return Err(format!("不是目录: {path}"));
    }
    if !unix_access(Path::new(path), libc::R_OK | libc::X_OK) {
        return Err(format!("目录不能列出: {path}"));
    }
    Ok(())
}

pub(super) fn probe_read(path: &str) -> Result<(), ExecutionError> {
    let meta = std::fs::metadata(path).map_err(|_| ExecutionError::new(4037, format!("文件不存在: {path}")))?;
    if !meta.is_file() {
        return Err(ExecutionError::new(4037, format!("不是文件: {path}")));
    }
    if !unix_access(Path::new(path), libc::R_OK) {
        return Err(ExecutionError::new(4037, format!("文件不能读取: {path}")));
    }
    Ok(())
}

pub(super) fn probe_write(path: &str) -> Result<(), ExecutionError> {
    let parent = Path::new(path).parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent_text = parent.to_string_lossy();
    let meta = std::fs::metadata(parent).map_err(|_| ExecutionError::new(4037, format!("目录不存在: {parent_text}")))?;
    if !meta.is_dir() {
        return Err(ExecutionError::new(4037, format!("不是目录: {parent_text}")));
    }
    if !unix_access(parent, libc::W_OK | libc::X_OK) {
        return Err(ExecutionError::new(4037, format!("目录不能写入: {parent_text}")));
    }
    match std::fs::metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ExecutionError::new(4037, err.to_string())),
        Ok(file) if !file.is_file() => Err(ExecutionError::new(4037, format!("不是文件: {path}"))),
        Ok(_) if !unix_access(Path::new(path), libc::W_OK) => Err(ExecutionError::new(4037, format!("文件不能写入: {path}"))),
        Ok(_) => Ok(()),
    }
}

fn unix_access(path: &Path, mode: i32) -> bool {
    let Ok(text) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    unsafe { libc::access(text.as_ptr(), mode) == 0 }
}
