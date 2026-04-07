/// 加解密工具模块
///
/// 对标 Node.js SDK src/crypto.ts
/// 提供文件加解密相关的功能函数，使用 AES-256-CBC 解密

use aes::Aes256;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use cbc::cipher::{BlockDecryptMut, KeyIvInit};

use crate::types::SdkError;

type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// 使用 AES-256-CBC 解密文件
///
/// # Arguments
/// * `encrypted_data` - 加密的文件数据
/// * `aes_key` - Base64 编码的 AES-256 密钥
///
/// # Returns
/// 解密后的文件数据
///
/// # Errors
/// 返回 SdkError::Decryption 当解密失败时
pub fn decrypt_file(encrypted_data: &[u8], aes_key: &str) -> Result<Vec<u8>, SdkError> {
    if encrypted_data.is_empty() {
        return Err(SdkError::Decryption(
            "decrypt_file: encrypted_data is empty or not provided".to_string(),
        ));
    }

    if aes_key.is_empty() {
        return Err(SdkError::Decryption(
            "decrypt_file: aes_key must be a non-empty string".to_string(),
        ));
    }

    // 将 Base64 编码的 aesKey 解码为 bytes
    // Node.js 的 Buffer.from(str, 'base64') 会自动容错处理缺少的 '=' padding，
    // 但 Python 的 base64.b64decode 严格要求长度是 4 的倍数，需要手动补齐。
    let padded_aes_key = if aes_key.len() % 4 != 0 {
        format!("{}{}", aes_key, "=".repeat(4 - aes_key.len() % 4))
    } else {
        aes_key.to_string()
    };

    let key = BASE64
        .decode(&padded_aes_key)
        .map_err(|e| SdkError::Decryption(format!("Failed to decode aes_key: {}", e)))?;

    if key.len() != 32 {
        return Err(SdkError::Decryption(format!(
            "Invalid aes_key length: expected 32 bytes, got {}",
            key.len()
        )));
    }

    // IV 取 aesKey 解码后的前 16 字节
    let iv = &key[..16];
    let key_array: &[u8; 32] = key.as_slice().try_into().map_err(|_| {
        SdkError::Decryption("Failed to convert key to array".to_string())
    })?;
    let iv_array: &[u8; 16] = iv.try_into().map_err(|_| {
        SdkError::Decryption("Failed to convert iv to array".to_string())
    })?;

    // 确保加密数据长度是 AES block size (16 字节) 的倍数
    // Node.js 的 setAutoPadding(false) 不会对不对齐的数据报错，
    // 但 Rust 的 cryptography 库会抛出 "Incorrect padding"。
    // 这里手动补零对齐，后续通过 PKCS#7 去除 padding 来获得正确数据。
    let block_size = 16;
    let remainder = encrypted_data.len() % block_size;
    let mut padded_data = if remainder != 0 {
        let mut padded = encrypted_data.to_vec();
        padded.extend(std::iter::repeat(0).take(block_size - remainder));
        padded
    } else {
        encrypted_data.to_vec()
    };

    // 解密（使用 PKCS7 padding）
    let cipher = Aes256CbcDec::new(key_array.into(), iv_array.into());
    let decrypted = cipher
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut padded_data)
        .map_err(|e| {
            SdkError::Decryption(format!(
                "Decryption failed: {}. This may indicate corrupted data or an incorrect aesKey.",
                e
            ))
        })?;

    // 手动去除 PKCS#7 填充（支持 32 字节 block）
    if decrypted.is_empty() {
        return Err(SdkError::Decryption("Decrypted data is empty".to_string()));
    }

    Ok(decrypted.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::Aes256;
    use cbc::cipher::{BlockEncryptMut, KeyIvInit};
    use cbc::cipher::block_padding::Pkcs7;

    type Aes256CbcEnc = cbc::Encryptor<Aes256>;

    fn encrypt_file(data: &[u8], aes_key: &str) -> Result<Vec<u8>, SdkError> {
        let padded_aes_key = if aes_key.len() % 4 != 0 {
            format!("{}{}", aes_key, "=".repeat(4 - aes_key.len() % 4))
        } else {
            aes_key.to_string()
        };

        let key = BASE64
            .decode(&padded_aes_key)
            .map_err(|e| SdkError::Decryption(format!("Failed to decode aes_key: {}", e)))?;

        let iv = &key[..16];
        let key_array: &[u8; 32] = key.as_slice().try_into().map_err(|_| {
            SdkError::Decryption("Failed to convert key to array".to_string())
        })?;
        let iv_array: &[u8; 16] = iv.try_into().map_err(|_| {
            SdkError::Decryption("Failed to convert iv to array".to_string())
        })?;

        let mut buf = data.to_vec();
        let data_len = data.len();
        // 扩展缓冲区以容纳 padding
        buf.resize(((data_len + 15) / 16) * 16 + 16, 0);

        let cipher = Aes256CbcEnc::new(key_array.into(), iv_array.into());
        let encrypted = cipher
            .encrypt_padded_mut::<Pkcs7>(&mut buf, data_len)
            .map_err(|e| SdkError::Decryption(format!("Encryption failed: {}", e)))?
            .to_vec();

        Ok(encrypted)
    }

    #[test]
    fn test_decrypt_file() {
        // 生成一个 32 字节的随机密钥（Base64 编码）
        let key_bytes: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let aes_key = BASE64.encode(&key_bytes);

        // 测试数据
        let original_data = b"Hello, World! This is a test.";

        // 加密
        let encrypted = encrypt_file(original_data, &aes_key).unwrap();

        // 解密
        let decrypted = decrypt_file(&encrypted, &aes_key).unwrap();

        assert_eq!(original_data.to_vec(), decrypted);
    }

    #[test]
    fn test_decrypt_file_empty_data() {
        let key_bytes: [u8; 32] = [0u8; 32];
        let aes_key = BASE64.encode(&key_bytes);
        assert!(decrypt_file(&[], &aes_key).is_err());
    }

    #[test]
    fn test_decrypt_file_empty_key() {
        let data = b"test";
        assert!(decrypt_file(data, "").is_err());
    }
}
