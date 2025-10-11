/*
 * 文件操作 (File Operations)
 *
 * 处理文件的读取、写入和遍历。
 * 严格禁止使用 ../、.../ 和 ./
 */

use std::fs;
use std::path::{Path, PathBuf};
use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::fmt;

/// 文件操作错误
#[derive(Debug)]
pub struct FileOpError {
    info: ErrorInfo,
}

impl FileOpError {
    /// 创建新的文件操作错误
    pub fn new(code: u32, message: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message)
                .with_category(ErrorCategory::Io)
                .with_severity(ErrorSeverity::Error),
        }
    }
}

impl fmt::Display for FileOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}

impl std::error::Error for FileOpError {}

/// 文件操作
pub struct FileOperations;

impl FileOperations {
    /// 验证路径安全性
    ///
    /// 禁止使用 ../、.../ 和 ./
    pub fn validate_path(path: &str) -> Result<(), FileOpError> {
        if path.contains("../") || path.contains(".../") || path.starts_with("./") {
            return Err(FileOpError::new(
                3001,
                format!("路径不安全: {}，禁止使用 ../、.../ 和 ./", path),
            ));
        }
        Ok(())
    }
    
    /// 读取文件内容
    ///
    /// # 参数
    /// - `path`: 文件路径
    ///
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 文件内容
    /// - `Err(FileOpError)`: 读取失败
    pub fn read_file(path: &str) -> Result<Vec<u8>, FileOpError> {
        Self::validate_path(path)?;
        
        fs::read(path).map_err(|e| {
            FileOpError::new(3002, format!("读取文件失败 {}: {}", path, e))
        })
    }
    
    /// 写入文件内容
    ///
    /// # 参数
    /// - `path`: 文件路径
    /// - `content`: 文件内容
    ///
    /// # 返回值
    /// - `Ok(())`: 写入成功
    /// - `Err(FileOpError)`: 写入失败
    pub fn write_file(path: &str, content: &[u8]) -> Result<(), FileOpError> {
        Self::validate_path(path)?;
        
        // 确保父目录存在
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(|e| {
                FileOpError::new(3003, format!("创建目录失败 {}: {}", parent.display(), e))
            })?;
        }
        
        fs::write(path, content).map_err(|e| {
            FileOpError::new(3004, format!("写入文件失败 {}: {}", path, e))
        })
    }
    
    /// 递归获取目录中的所有文件
    ///
    /// # 参数
    /// - `dir`: 目录路径
    /// - `exclude`: 要排除的路径列表
    ///
    /// # 返回值
    /// - `Ok(Vec<PathBuf>)`: 文件路径列表
    /// - `Err(FileOpError)`: 操作失败
    pub fn list_files_recursive(
        dir: &str,
        exclude: &[String],
    ) -> Result<Vec<PathBuf>, FileOpError> {
        Self::validate_path(dir)?;
        
        let mut files = Vec::new();
        Self::list_files_internal(Path::new(dir), exclude, &mut files)?;
        Ok(files)
    }
    
    /// 内部递归函数
    fn list_files_internal(
        dir: &Path,
        exclude: &[String],
        files: &mut Vec<PathBuf>,
    ) -> Result<(), FileOpError> {
        let entries = fs::read_dir(dir).map_err(|e| {
            FileOpError::new(3005, format!("读取目录失败 {}: {}", dir.display(), e))
        })?;
        
        for entry in entries {
            let entry = entry.map_err(|e| {
                FileOpError::new(3006, format!("读取目录条目失败: {}", e))
            })?;
            
            let path = entry.path();
            
            // 获取文件/目录名称（不包含父路径）
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            // 检查是否应排除
            let should_exclude = exclude.iter().any(|ex| name.contains(ex));
            
            if should_exclude {
                continue;
            }
            
            if path.is_dir() {
                Self::list_files_internal(&path, exclude, files)?;
            } else if path.is_file() {
                files.push(path);
            }
        }
        
        Ok(())
    }
    
    /// 检查文件是否存在
    pub fn exists(path: &str) -> bool {
        Path::new(path).exists()
    }
    
    /// 检查是否是文件
    pub fn is_file(path: &str) -> bool {
        Path::new(path).is_file()
    }
    
    /// 检查是否是目录
    pub fn is_dir(path: &str) -> bool {
        Path::new(path).is_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_validate_path_safe() {
        assert!(FileOperations::validate_path("path/to/file.txt").is_ok());
        assert!(FileOperations::validate_path("/absolute/path").is_ok());
    }

    #[test]
    fn test_validate_path_unsafe() {
        assert!(FileOperations::validate_path("../path").is_err());
        assert!(FileOperations::validate_path(".../path").is_err());
        assert!(FileOperations::validate_path("./path").is_err());
        assert!(FileOperations::validate_path("path/../file").is_err());
    }

    #[test]
    fn test_read_write_file() {
        let test_path = "/tmp/dake_test_file.txt";
        let content = b"Test content";
        
        // 写入
        FileOperations::write_file(test_path, content).expect("写入失败");
        
        // 读取
        let read_content = FileOperations::read_file(test_path).expect("读取失败");
        
        assert_eq!(content, read_content.as_slice());
        
        // 清理
        let _ = fs::remove_file(test_path);
    }

    #[test]
    fn test_list_files_recursive() {
        // 创建测试目录结构
        let test_dir = "/tmp/dake_test_dir";
        let _ = fs::create_dir_all(format!("{}/subdir", test_dir));
        
        // 创建测试文件
        let mut file1 = fs::File::create(format!("{}/file1.txt", test_dir)).unwrap();
        file1.write_all(b"content1").unwrap();
        
        let mut file2 = fs::File::create(format!("{}/subdir/file2.txt", test_dir)).unwrap();
        file2.write_all(b"content2").unwrap();
        
        // 列出文件
        let files = FileOperations::list_files_recursive(test_dir, &[]).expect("列出文件失败");
        
        assert_eq!(files.len(), 2);
        
        // 清理
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_list_files_with_exclude() {
        let test_dir = "/tmp/dake_test_exclude";
        
        // 清理之前的测试
        let _ = fs::remove_dir_all(test_dir);
        
        // 创建目录结构
        let _ = fs::create_dir_all(format!("{}/include", test_dir));
        let _ = fs::create_dir_all(format!("{}/exclude", test_dir));
        
        // 创建测试文件
        let mut file1 = fs::File::create(format!("{}/include/file1.txt", test_dir)).unwrap();
        file1.write_all(b"content1").unwrap();
        
        let mut file2 = fs::File::create(format!("{}/exclude/file2.txt", test_dir)).unwrap();
        file2.write_all(b"content2").unwrap();
        
        // 列出文件，排除 exclude 目录
        let files = FileOperations::list_files_recursive(
            test_dir,
            &["exclude".to_string()],
        ).expect("列出文件失败");
        
        // 应该只有一个文件（file1.txt），不包含 exclude 目录中的文件
        if files.len() != 1 {
            eprintln!("期望 1 个文件，实际得到 {} 个:", files.len());
            for f in &files {
                eprintln!("  - {}", f.display());
            }
        }
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("file1.txt"));
        
        // 清理
        let _ = fs::remove_dir_all(test_dir);
    }
}
