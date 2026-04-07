/// 通用工具方法
///
/// 对标 Node.js SDK src/utils.ts

use rand::Rng;

/// 生成随机字符串
///
/// # Arguments
/// * `length` - 随机字符串长度，默认 8
///
/// # Returns
/// 随机十六进制字符串
pub fn generate_random_string(length: usize) -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..(length + 1) / 2).map(|_| rng.random()).collect();
    hex::encode(bytes)[..length].to_string()
}

/// 生成唯一请求 ID
///
/// 格式：{prefix}_{timestamp}_{random}
///
/// # Arguments
/// * `prefix` - 前缀，通常为 cmd 名称
///
/// # Returns
/// 唯一请求 ID
pub fn generate_req_id(prefix: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let random_str = generate_random_string(8);
    format!("{}_{}_{}", prefix, timestamp, random_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_string() {
        let s1 = generate_random_string(8);
        let s2 = generate_random_string(8);
        assert_eq!(s1.len(), 8);
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_generate_req_id() {
        let id1 = generate_req_id("test");
        let id2 = generate_req_id("test");
        assert!(id1.starts_with("test_"));
        assert_ne!(id1, id2);
    }
}
