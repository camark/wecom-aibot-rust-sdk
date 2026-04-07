/// 企业微信智能机器人 SDK 类型定义
///
/// 对标 Node.js SDK src/types/ 目录下的全部类型

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

// ========== 错误类型 ==========

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("SDK error: {0}")]
    Internal(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for SdkError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        SdkError::WebSocket(err.to_string())
    }
}

impl From<reqwest::Error> for SdkError {
    fn from(err: reqwest::Error) -> Self {
        SdkError::Http(err.to_string())
    }
}

impl From<serde_json::Error> for SdkError {
    fn from(err: serde_json::Error) -> Self {
        SdkError::Serialization(err.to_string())
    }
}

// ========== 通用基础类型 (common.ts) ==========

/// 日志接口
pub trait Logger: Send + Sync {
    fn debug(&self, message: &str);
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
    fn error(&self, message: &str);
}

// ========== 配置类型 (config.ts) ==========

/// WSClient 配置选项
#[derive(Clone)]
pub struct WSClientOptions {
    /// 机器人 ID（在企业微信后台获取）
    pub bot_id: String,

    /// 机器人 Secret（在企业微信后台获取）
    pub secret: String,

    /// WebSocket 重连基础延迟（毫秒），实际延迟按指数退避递增，默认 1000
    pub reconnect_interval: u64,

    /// 最大重连次数，默认 10，设为 -1 表示无限重连
    pub max_reconnect_attempts: i32,

    /// 心跳间隔（毫秒），默认 30000
    pub heartbeat_interval: u64,

    /// 请求超时时间（毫秒），默认 10000
    pub request_timeout: u64,

    /// 自定义 WebSocket 连接地址，默认 wss://openws.work.weixin.qq.com
    pub ws_url: Option<String>,

    /// 自定义日志实例
    pub logger: Option<Arc<dyn Logger>>,
}

impl Default for WSClientOptions {
    fn default() -> Self {
        Self {
            bot_id: String::new(),
            secret: String::new(),
            reconnect_interval: 1000,
            max_reconnect_attempts: 10,
            heartbeat_interval: 30000,
            request_timeout: 10000,
            ws_url: None,
            logger: None,
        }
    }
}

impl WSClientOptions {
    pub fn new(bot_id: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            bot_id: bot_id.into(),
            secret: secret.into(),
            ..Default::default()
        }
    }
}

// ========== WebSocket 命令常量 (api.ts) ==========

/// WebSocket 命令类型常量
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsCmd;

impl WsCmd {
    // ========== 开发者 → 企业微信 ==========
    pub const SUBSCRIBE: &'static str = "aibot_subscribe";
    pub const HEARTBEAT: &'static str = "ping";
    pub const RESPONSE: &'static str = "aibot_respond_msg";
    pub const RESPONSE_WELCOME: &'static str = "aibot_respond_welcome_msg";
    pub const RESPONSE_UPDATE: &'static str = "aibot_respond_update_msg";
    pub const SEND_MSG: &'static str = "aibot_send_msg";

    // ========== 企业微信 → 开发者 ==========
    pub const CALLBACK: &'static str = "aibot_msg_callback";
    pub const EVENT_CALLBACK: &'static str = "aibot_event_callback";
}

// ========== 消息类型枚举 (message.ts) ==========

/// 消息类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "mixed")]
    Mixed,
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "file")]
    File,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::Text => write!(f, "text"),
            MessageType::Image => write!(f, "image"),
            MessageType::Mixed => write!(f, "mixed"),
            MessageType::Voice => write!(f, "voice"),
            MessageType::File => write!(f, "file"),
        }
    }
}

// ========== 事件类型枚举 (event.ts) ==========

/// 事件类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// 进入会话事件：用户当天首次进入机器人单聊会话
    #[serde(rename = "enter_chat")]
    EnterChat,
    /// 模板卡片事件：用户点击模板卡片按钮
    #[serde(rename = "template_card_event")]
    TemplateCardEvent,
    /// 用户反馈事件：用户对机器人回复进行反馈
    #[serde(rename = "feedback_event")]
    FeedbackEvent,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventType::EnterChat => write!(f, "enter_chat"),
            EventType::TemplateCardEvent => write!(f, "template_card_event"),
            EventType::FeedbackEvent => write!(f, "feedback_event"),
        }
    }
}

// ========== 模板卡片类型枚举 (api.ts) ==========

/// 卡片类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCardType {
    /// 文本通知模版卡片
    #[serde(rename = "text_notice")]
    TextNotice,
    /// 图文展示模版卡片
    #[serde(rename = "news_notice")]
    NewsNotice,
    /// 按钮交互模版卡片
    #[serde(rename = "button_interaction")]
    ButtonInteraction,
    /// 投票选择模版卡片
    #[serde(rename = "vote_interaction")]
    VoteInteraction,
    /// 多项选择模版卡片
    #[serde(rename = "multiple_interaction")]
    MultipleInteraction,
}

// ========== WebSocket 帧结构 (api.ts) ==========

/// WebSocket 帧结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsFrame {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,

    pub headers: WsFrameHeaders,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}

impl WsFrame {
    pub fn new(cmd: impl Into<String>, headers: WsFrameHeaders) -> Self {
        Self {
            cmd: Some(cmd.into()),
            headers,
            body: None,
            errcode: None,
            errmsg: None,
        }
    }

    pub fn with_body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }
}

/// WebSocket 帧头部
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsFrameHeaders {
    pub req_id: String,
}

impl WsFrameHeaders {
    pub fn new(req_id: impl Into<String>) -> Self {
        Self {
            req_id: req_id.into(),
        }
    }
}
