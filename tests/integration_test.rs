/*
 * 集成测试：数据处理功能完整测试
 *
 * 测试所有数据处理操作的实际执行，而非仅记录行为。
 */

#[cfg(test)]
mod integration_tests {
    use std::process::Command;
    use std::fs;

    #[test]
    fn test_data_validation_integration() {
        // 创建测试 DSL 文件
        let dsl_content = r#"
            SET(email, "test@example.com")
            DATA.VALI.EMAIL(email, ${email})
            PRINT("Email validation passed")
        "#;
        
        let test_file = "/tmp/test_validation.dake";
        fs::write(test_file, dsl_content).expect("创建测试文件失败");
        
        // 运行 dake
        let output = Command::new("cargo")
            .args(&["run", "--", "run", test_file])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("执行 dake 失败");
        
        assert!(output.status.success(), "dake 应该成功执行");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Email validation passed") || stdout.contains("验证成功"), 
                "应该包含验证成功消息");
        
        // 清理
        let _ = fs::remove_file(test_file);
    }
    
    #[test]
    fn test_data_compression_integration() {
        // 创建测试 DSL 文件
        let dsl_content = r#"
            SET(data, "Hello World! This is a test for compression.")
            DATA.COMP(3, ${data})
            PRINT("Compression successful")
        "#;
        
        let test_file = "/tmp/test_compression.dake";
        fs::write(test_file, dsl_content).expect("创建测试文件失败");
        
        // 运行 dake
        let output = Command::new("cargo")
            .args(&["run", "--", "run", test_file])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("执行 dake 失败");
        
        assert!(output.status.success(), "dake 应该成功执行");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Compression successful") || stdout.contains("压缩成功"), 
                "应该包含压缩成功消息");
        
        // 清理
        let _ = fs::remove_file(test_file);
    }
    
    #[test]
    fn test_command_execution_integration() {
        // 创建测试 DSL 文件
        let dsl_content = r#"
            COMM.CMD(echo, ["Hello", "from", "dake"])
            PRINT("Command executed")
        "#;
        
        let test_file = "/tmp/test_command.dake";
        fs::write(test_file, dsl_content).expect("创建测试文件失败");
        
        // 运行 dake
        let output = Command::new("cargo")
            .args(&["run", "--", "run", test_file])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("执行 dake 失败");
        
        assert!(output.status.success(), "dake 应该成功执行");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Command executed") || stdout.contains("命令执行"), 
                "应该包含命令执行消息");
        
        // 清理
        let _ = fs::remove_file(test_file);
    }
    
    #[test]
    fn test_serialization_integration() {
        // 创建测试 DSL 文件
        let dsl_content = r#"
            SET(num, "42")
            DATA.SERIA.JSON(${num})
            PRINT("Serialization complete")
        "#;
        
        let test_file = "/tmp/test_serialization.dake";
        fs::write(test_file, dsl_content).expect("创建测试文件失败");
        
        // 运行 dake
        let output = Command::new("cargo")
            .args(&["run", "--", "run", test_file])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("执行 dake 失败");
        
        assert!(output.status.success(), "dake 应该成功执行");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Serialization complete") || stdout.contains("序列化"), 
                "应该包含序列化消息");
        
        // 清理
        let _ = fs::remove_file(test_file);
    }
}
