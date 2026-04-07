/// 企业微信 API 客户端
///
/// 对标 Node.js SDK src/api.ts
/// 仅负责文件下载等 HTTP 辅助功能，消息收发均走 WebSocket 通道。

use std::sync::Arc;
use std::time::Duration;

use crate::types::{Logger, SdkError};

/// 企业微信 API 客户端
pub struct WeComApiClient {
    client: reqwest::Client,
    logger: Arc<dyn Logger>,
}

impl WeComApiClient {
    pub fn new(logger: Arc<dyn Logger>, timeout_ms: u64) -> Self {
        let timeout = Duration::from_millis(timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();

        Self { client, logger }
    }

    /// 下载文件（返回原始 bytes 及文件名）
    ///
    /// # Arguments
    /// * `url` - 文件下载地址
    ///
    /// # Returns
    /// (文件数据，文件名)
    pub async fn download_file_raw(
        &self,
        url: &str,
    ) -> Result<(Vec<u8>, Option<String>), SdkError> {
        self.logger.info("Downloading file...");

        let response = self.client.get(url).send().await?;
        let response = response.error_for_status()?;

        // 从 Content-Disposition 头中解析文件名
        let filename = response
            .headers()
            .get("Content-Disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(Self::parse_filename);

        let data = response.bytes().await?.to_vec();

        self.logger.info("File downloaded successfully");
        Ok((data, filename))
    }

    /// 从 Content-Disposition 头中解析文件名
    fn parse_filename(content_disposition: &str) -> Option<String> {
        // 优先匹配 filename*=UTF-8''xxx 格式（RFC 5987）
        let utf8_pattern = regex::Regex::new(r"filename\*=UTF-8''([^;\s]+)").ok()?;
        if let Some(caps) = utf8_pattern.captures(content_disposition) {
            if let Some(encoded) = caps.get(1) {
                return Some(
                    urlencoding::decode(encoded.as_str())
                        .unwrap_or_else(|_| encoded.as_str().to_string().into())
                        .to_string(),
                );
            }
        }

        // 匹配 filename="xxx" 或 filename=xxx 格式
        let fallback_pattern = regex::Regex::new(r#"filename="?([^";\s]+)"?"#).ok()?;
        if let Some(caps) = fallback_pattern.captures(content_disposition) {
            if let Some(filename) = caps.get(1) {
                return Some(
                    urlencoding::decode(filename.as_str())
                        .unwrap_or_else(|_| filename.as_str().to_string().into())
                        .to_string(),
                );
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::DefaultLogger;

    #[test]
    fn test_parse_filename_simple() {
        let result = WeComApiClient::parse_filename("attachment; filename=\"test.txt\"");
        assert_eq!(result, Some("test.txt".to_string()));
    }

    #[test]
    fn test_parse_filename_utf8() {
        let result =
            WeComApiClient::parse_filename("attachment; filename*=UTF-8''%E6%B5%8B%E8%AF%95.txt");
        assert_eq!(result, Some("测试.txt".to_string()));
    }

    #[test]
    fn test_parse_filename_no_quotes() {
        let result = WeComApiClient::parse_filename("attachment; filename=test.txt");
        assert_eq!(result, Some("test.txt".to_string()));
    }

    #[test]
    fn test_parse_filename_none() {
        let result = WeComApiClient::parse_filename("inline");
        assert_eq!(result, None);
    }
}
