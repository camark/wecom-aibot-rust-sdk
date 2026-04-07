//! 企业微信智能机器人 Rust SDK
//!
//! 基于 WebSocket 长连接通道，提供消息收发、流式回复、模板卡片、事件回调、文件下载解密等核心能力。
//!
//! # 快速开始
//!
//! ```no_run
//! use wecom_aibot_rust_sdk::{WSClient, WSClientOptions};
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = WSClient::new(
//!         WSClientOptions::new("your-bot-id", "your-bot-secret")
//!     );
//!
//!     // 监听连接事件
//!     client.on_connected(|| {
//!         println!("WebSocket 已连接");
//!     });
//!
//!     // 监听认证成功
//!     client.on_authenticated(|| {
//!         println!("认证成功");
//!     });
//!
//!     // 连接
//!     client.connect().await.unwrap();
//!
//!     // 保持运行
//!     tokio::signal::ctrl_c().await.ok();
//!
//!     client.disconnect();
//! }
//! ```
//!
//! # 特性
//!
//! - 🔗 **WebSocket 长连接** — 基于 `wss://openws.work.weixin.qq.com` 内置默认地址，开箱即用
//! - 🔐 **自动认证** — 连接建立后自动发送认证帧（bot_id + secret）
//! - 💓 **心跳保活** — 自动维护心跳，连续未收到 ack 时自动判定连接异常
//! - 🔄 **断线重连** — 指数退避重连策略（1s → 2s → 4s → ... → 30s 上限），支持自定义最大重连次数
//! - 📨 **消息分发** — 自动解析消息类型并触发对应事件（text / image / mixed / voice / file）
//! - 🌊 **流式回复** — 内置流式回复方法，支持 Markdown 和图文混排
//! - 🃏 **模板卡片** — 支持回复模板卡片消息、流式 + 卡片组合回复、更新卡片
//! - 📤 **主动推送** — 支持向指定会话主动发送 Markdown 或模板卡片消息，无需依赖回调帧
//! - 📡 **事件回调** — 支持进入会话、模板卡片按钮点击、用户反馈等事件
//! - ⏩ **串行回复队列** — 同一 req_id 的回复消息串行发送，自动等待回执
//! - 🔑 **文件下载解密** — 内置 AES-256-CBC 文件解密，每个图片/文件消息自带独立的 aeskey
//! - 🪵 **可插拔日志** — 支持自定义 Logger，内置带时间戳的 DefaultLogger
//! - 🦀 **tokio 原生** — 基于 Rust tokio 异步运行时，支持 async/await

extern crate alloc;

pub mod api;
pub mod client;
pub mod crypto_utils;
pub mod logger;
pub mod message_handler;
pub mod types;
pub mod utils;
pub mod ws;

// 重新导出常用类型
pub use client::WSClient;
pub use logger::DefaultLogger;
pub use types::{
    EventType, Logger, MessageType, SdkError, TemplateCardType, WsCmd, WsFrame, WsFrameHeaders,
    WSClientOptions,
};
pub use utils::generate_req_id;

/// SDK 版本号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
