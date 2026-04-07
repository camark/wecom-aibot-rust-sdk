/// 默认日志实现
///
/// 对标 Node.js SDK src/logger.ts
/// 带有日志级别和时间戳的控制台日志

use crate::types::Logger;
use std::io::{self, Write};

/// 默认日志实现，带有日志级别和时间戳的控制台日志
pub struct DefaultLogger {
    prefix: String,
}

impl DefaultLogger {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    fn format_time() -> String {
        let now = chrono::Utc::now();
        now.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string()
    }

    fn log(&self, level: &str, message: &str) {
        let line = format!("[{}] [{}] [{}] {}", Self::format_time(), self.prefix, level, message);
        writeln!(io::stderr(), "{}", line).ok();
    }
}

impl Default for DefaultLogger {
    fn default() -> Self {
        Self::new("AiBotSDK")
    }
}

impl Logger for DefaultLogger {
    fn debug(&self, message: &str) {
        self.log("DEBUG", message);
    }

    fn info(&self, message: &str) {
        self.log("INFO", message);
    }

    fn warn(&self, message: &str) {
        self.log("WARN", message);
    }

    fn error(&self, message: &str) {
        self.log("ERROR", message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger() {
        let logger = DefaultLogger::default();
        logger.info("Test info");
        logger.debug("Test debug");
        logger.warn("Test warn");
        logger.error("Test error");
    }
}
