/*
 * 加密操作 (Crypto Operations)
 *
 * 使用 XChaCha20-Poly1305 进行文件加密。
 * 提供 deterministic 模式处理密钥。
 */

use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce,
};
use chacha20poly1305::aead::rand_core::RngCore;
use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::fmt;

/// 加密错误
#[derive(Debug)]
#[allow(dead_code)]
pub struct CryptoError {
    info: ErrorInfo,
}

impl CryptoError {
    /// 创建新的加密错误
    #[allow(dead_code)]
    pub fn new(code: u32, message: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message)
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error),
        }
    }
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}

impl std::error::Error for CryptoError {}

/// 加密操作
#[allow(dead_code)]
pub struct CryptoOperations {
    /// 密钥（32 字节）
    key: [u8; 32],
    
    /// deterministic 模式
    deterministic: bool,
}

#[allow(dead_code)]
impl CryptoOperations {
    /// 创建新的加密操作实例
    ///
    /// # 参数
    /// - `deterministic`: 是否使用确定性模式
    ///
    /// # 返回值
    /// - `(CryptoOperations, Vec<u8>)`: 操作实例和生成的密钥
    pub fn new(deterministic: bool) -> (Self, Vec<u8>) {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        
        let ops = Self {
            key,
            deterministic,
        };
        
        (ops, key.to_vec())
    }
    
    /// 使用给定密钥创建加密操作实例
    ///
    /// # 参数
    /// - `key`: 32 字节密钥
    /// - `deterministic`: 是否使用确定性模式
    ///
    /// # 返回值
    /// - `Ok(CryptoOperations)`: 成功创建的实例
    /// - `Err(CryptoError)`: 密钥长度不正确
    pub fn with_key(key: &[u8], deterministic: bool) -> Result<Self, CryptoError> {
        if key.len() != 32 {
            return Err(CryptoError::new(
                2001,
                format!("密钥长度必须为 32 字节，当前为 {} 字节", key.len()),
            ));
        }
        
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(key);
        
        Ok(Self {
            key: key_array,
            deterministic,
        })
    }
    
    /// 加密数据
    ///
    /// # 参数
    /// - `data`: 要加密的数据
    ///
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 加密后的数据（nonce + ciphertext）
    /// - `Err(CryptoError)`: 加密失败
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = XChaCha20Poly1305::new(&self.key.into());
        
        // 生成 nonce
        let nonce = if self.deterministic {
            // deterministic 模式：使用数据的哈希作为 nonce
            self.generate_deterministic_nonce(data)
        } else {
            // 随机 nonce
            let mut nonce_bytes = [0u8; 24];
            OsRng.fill_bytes(&mut nonce_bytes);
            XNonce::from(nonce_bytes)
        };
        
        // 加密
        let ciphertext = cipher
            .encrypt(&nonce, data)
            .map_err(|e| CryptoError::new(2002, format!("加密失败: {}", e)))?;
        
        // 返回 nonce + ciphertext
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);
        
        Ok(result)
    }
    
    /// 解密数据
    ///
    /// # 参数
    /// - `encrypted_data`: 加密的数据（nonce + ciphertext）
    ///
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 解密后的数据
    /// - `Err(CryptoError)`: 解密失败
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if encrypted_data.len() < 24 {
            return Err(CryptoError::new(
                2003,
                "加密数据过短，缺少 nonce".to_string(),
            ));
        }
        
        let cipher = XChaCha20Poly1305::new(&self.key.into());
        
        // 提取 nonce 和 ciphertext
        let (nonce_bytes, ciphertext) = encrypted_data.split_at(24);
        let nonce = XNonce::from_slice(nonce_bytes);
        
        // 解密
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CryptoError::new(2004, format!("解密失败: {}", e)))?;
        
        Ok(plaintext)
    }
    
    /// 生成确定性 nonce
    ///
    /// 使用 BLAKE3 哈希函数基于密钥和数据生成固定的 nonce
    ///
    /// # 参数
    /// - `data`: 要生成 nonce 的数据
    ///
    /// # 返回
    /// 24 字节的 nonce
    fn generate_deterministic_nonce(&self, data: &[u8]) -> XNonce {
        // 使用 BLAKE3 keyed hash 生成确定性 nonce
        // 这确保了相同的密钥和数据总是产生相同的 nonce
        let mut hasher = blake3::Hasher::new_keyed(&self.key);
        hasher.update(data);
        let hash = hasher.finalize();
        
        // 取前 24 字节作为 nonce
        let mut nonce_bytes = [0u8; 24];
        nonce_bytes.copy_from_slice(&hash.as_bytes()[..24]);
        
        XNonce::from(nonce_bytes)
    }
    
    /// 获取密钥
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let (ops, _key) = CryptoOperations::new(false);
        let data = b"Hello, World!";
        
        let encrypted = ops.encrypt(data).expect("加密失败");
        let decrypted = ops.decrypt(&encrypted).expect("解密失败");
        
        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_deterministic_mode() {
        let (ops, key) = CryptoOperations::new(true);
        let data = b"Test data";
        
        // 多次加密相同数据应得到相同结果
        let encrypted1 = ops.encrypt(data).expect("加密失败");
        let encrypted2 = ops.encrypt(data).expect("加密失败");
        
        assert_eq!(encrypted1, encrypted2);
        
        // 解密应正常工作
        let decrypted = ops.decrypt(&encrypted1).expect("解密失败");
        assert_eq!(data, decrypted.as_slice());
        
        // 使用相同密钥创建新实例应能解密
        let ops2 = CryptoOperations::with_key(&key, true).expect("创建实例失败");
        let decrypted2 = ops2.decrypt(&encrypted1).expect("解密失败");
        assert_eq!(data, decrypted2.as_slice());
    }

    #[test]
    fn test_wrong_key() {
        let (ops1, _) = CryptoOperations::new(false);
        let (ops2, _) = CryptoOperations::new(false);
        
        let data = b"Secret data";
        let encrypted = ops1.encrypt(data).expect("加密失败");
        
        // 使用错误的密钥解密应失败
        let result = ops2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_key_length() {
        let short_key = vec![0u8; 16];
        let result = CryptoOperations::with_key(&short_key, false);
        assert!(result.is_err());
    }
}
