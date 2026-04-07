# wecom-aibot-rust-sdk (Rust)

企业微信智能机器人 Rust SDK —— 基于 WebSocket 长连接通道，提供消息收发、流式回复、模板卡片、事件回调、文件下载解密等核心能力。

> 本项目是 Python 版本 [@wecom/aibot-python-sdk](https://pypi.org/project/wecom-aibot-python-sdk/) 的 Rust 改写版本。

## ✨ 特性

- 🔗 **WebSocket 长连接** — 基于 `wss://openws.work.weixin.qq.com` 内置默认地址，开箱即用
- 🔐 **自动认证** — 连接建立后自动发送认证帧（bot_id + secret）
- 💓 **心跳保活** — 自动维护心跳，连续未收到 ack 时自动判定连接异常
- 🔄 **断线重连** — 指数退避重连策略（1s → 2s → 4s → ... → 30s 上限），支持自定义最大重连次数
- 📨 **消息分发** — 自动解析消息类型并触发对应事件（text / image / mixed / voice / file）
- 🌊 **流式回复** — 内置流式回复方法，支持 Markdown 和图文混排
- 🃏 **模板卡片** — 支持回复模板卡片消息、流式 + 卡片组合回复、更新卡片
- 📤 **主动推送** — 支持向指定会话主动发送 Markdown 或模板卡片消息，无需依赖回调帧
- 📡 **事件回调** — 支持进入会话、模板卡片按钮点击、用户反馈等事件
- ⏩ **串行回复队列** — 同一 req_id 的回复消息串行发送，自动等待回执
- 🔑 **文件下载解密** — 内置 AES-256-CBC 文件解密，每个图片/文件消息自带独立的 aeskey
- 🪵 **可插拔日志** — 支持自定义 Logger，内置带时间戳的 DefaultLogger
- 🦀 **tokio 原生** — 基于 Rust tokio 异步运行时，支持 async/await

## 📦 安装

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
wecom-aibot-rust-sdk = "1.0"
tokio = { version = "1", features = ["full"] }
```

**依赖：**
- Rust >= 1.70
- tokio >= 1.35
- tokio-tungstenite >= 0.21
- reqwest >= 0.11
- serde >= 1.0
- aes >= 0.8
- cbc >= 0.1

## ⚙️ 配置

```bash
# 复制示例配置文件
cp .env.example .env

# 编辑 .env 文件，填入真实配置
# WECHAT_BOT_ID=your-bot-id
# WECHAT_BOT_SECRET=your-bot-secret
```

## 🚀 快速开始

```rust
use dotenv::dotenv;
use std::env;
use tokio::time::{self, Duration};
use wecom_aibot_rust_sdk::{generate_req_id, WSClient, WSClientOptions};

#[tokio::main]
async fn main() {
    // 加载 .env 文件
    dotenv().ok();

    // 1. 创建客户端实例
    let client = WSClient::new(WSClientOptions::new(
        env::var("WECHAT_BOT_ID").unwrap(),
        env::var("WECHAT_BOT_SECRET").unwrap(),
    ));

    // 2. 监听认证成功
    client.on_authenticated(|| {
        println!("🔐 认证成功");
    });

    // 3. 监听文本消息并进行流式回复
    let client_text = client.clone();
    client.on_message_text(move |frame| {
        let client_text = client_text.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            let content = frame
                .body
                .as_ref()
                .and_then(|v| v.as_object())
                .and_then(|b| b.get("text"))
                .and_then(|v| v.as_object())
                .and_then(|t| t.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            println!("收到文本：{}", content);

            let stream_id = generate_req_id("stream");

            // 发送流式中间内容
            let _ = client_text
                .reply_stream(&frame, &stream_id, "正在思考中...", false, None, None)
                .await;

            // 发送最终结果
            time::sleep(Duration::from_secs(1)).await;
            let _ = client_text
                .reply_stream(
                    &frame,
                    &stream_id,
                    &format!("你好！你说的是：\"{}\"", content),
                    true,
                    None,
                    None,
                )
                .await;
        });
    });

    // 4. 监听进入会话事件（发送欢迎语）
    let client_enter = client.clone();
    client.on_event_enter_chat(move |frame| {
        let client_enter = client_enter.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            use serde_json::json;
            let _ = client_enter
                .reply_welcome(
                    &frame,
                    json!({
                        "msgtype": "text",
                        "text": {"content": "您好！我是智能助手，有什么可以帮您的吗？"}
                    }),
                )
                .await;
        });
    });

    // 5. 启动客户端
    println!("正在启动机器人...");
    if let Err(e) = client.connect().await {
        eprintln!("启动失败：{}", e);
        return;
    }

    println!("机器人启动成功，按 Ctrl+C 退出");

    // 等待退出信号
    tokio::signal::ctrl_c().await.ok();

    println!("\n正在停止机器人...");
    client.disconnect();
}
```

## 📖 API 文档

### `WSClient`

核心客户端类，提供连接管理、消息收发等功能。

#### 构造函数

```rust
let client = WSClient::new(WSClientOptions::new(bot_id, secret));
```

#### 配置选项

`WSClientOptions` 完整配置：

| 参数 | 类型 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `bot_id` | `String` | ✅ | — | 机器人 ID（企业微信后台获取） |
| `secret` | `String` | ✅ | — | 机器人 Secret（企业微信后台获取） |
| `reconnect_interval` | `u64` | — | `1000` | 重连基础延迟（毫秒），实际延迟按指数退避递增 |
| `max_reconnect_attempts` | `i32` | — | `10` | 最大重连次数（`-1` 表示无限重连） |
| `heartbeat_interval` | `u64` | — | `30000` | 心跳间隔（毫秒） |
| `request_timeout` | `u64` | — | `10000` | HTTP 请求超时时间（毫秒） |
| `ws_url` | `Option<String>` | — | `None` | 自定义 WebSocket 连接地址 |
| `logger` | `Option<Box<dyn Logger>>` | — | `None` | 自定义日志实例 |

```rust
let options = WSClientOptions {
    bot_id: "your-bot-id".to_string(),
    secret: "your-bot-secret".to_string(),
    reconnect_interval: 1000,
    max_reconnect_attempts: 10,
    heartbeat_interval: 30000,
    request_timeout: 10000,
    ws_url: None,
    logger: None,
};
```

#### 方法

| 方法 | 说明 | 返回值 |
| --- | --- | --- |
| `async connect()` | 建立 WebSocket 连接，连接后自动认证 | `Result<(), SdkError>` |
| `disconnect()` | 主动断开连接 | `()` |
| `async reply(frame, body, cmd)` | 通过 WebSocket 通道发送回复消息（通用方法） | `Result<WsFrame, SdkError>` |
| `async reply_stream(frame, stream_id, content, finish, msg_item, feedback)` | 发送流式文本回复（便捷方法，支持 Markdown） | `Result<WsFrame, SdkError>` |
| `async reply_welcome(frame, body)` | 发送欢迎语回复（支持文本或模板卡片格式），需在收到事件 5s 内调用 | `Result<WsFrame, SdkError>` |
| `async reply_template_card(frame, template_card, feedback)` | 回复模板卡片消息 | `Result<WsFrame, SdkError>` |
| `async reply_stream_with_card(frame, stream_id, content, finish, ...)` | 发送流式消息 + 模板卡片组合回复 | `Result<WsFrame, SdkError>` |
| `async update_template_card(frame, template_card, userids)` | 更新模板卡片，需在收到事件 5s 内调用 | `Result<WsFrame, SdkError>` |
| `async send_message(chatid, body)` | 主动发送消息（支持 Markdown 或模板卡片），无需依赖回调帧 | `Result<WsFrame, SdkError>` |
| `async download_file(url, aes_key)` | 下载文件并使用 AES 密钥解密 | `Result<(Vec<u8>, Option<String>), SdkError>` |

#### 属性

| 方法 | 说明 | 返回类型 |
| --- | --- | --- |
| `is_connected()` | 当前 WebSocket 连接状态 | `bool` |
| `api()` | 内部 API 客户端实例（高级用途） | `Arc<WeComApiClient>` |

### 事件监听

所有事件均通过 `on_*` 方法监听：

```rust
// 连接事件
client.on_connected(|| { println!("WebSocket 已连接"); });
client.on_authenticated(|| { println!("认证成功"); });
client.on_disconnected(|reason| { println!("连接已断开：{}", reason); });
client.on_reconnecting(|attempt| { println!("正在进行第 {} 次重连...", attempt); });
client.on_error(|error| { eprintln!("发生错误：{}", error); });

// 消息事件
client.on_message(|frame| { println!("收到消息：{:?}", frame); });
client.on_message_text(|frame| { println!("收到文本消息"); });
client.on_message_image(|frame| { println!("收到图片消息"); });
client.on_message_mixed(|frame| { println!("收到图文混排消息"); });
client.on_message_voice(|frame| { println!("收到语音消息"); });
client.on_message_file(|frame| { println!("收到文件消息"); });

// 事件回调
client.on_event(|frame| { println!("收到事件回调"); });
client.on_event_enter_chat(|frame| { println!("用户进入会话"); });
client.on_event_template_card(|frame| { println!("模板卡片事件"); });
client.on_event_feedback(|frame| { println!("用户反馈事件"); });
```

### `reply_stream` 详细说明

```rust
await client.reply_stream(
    &frame,              // 收到的原始 WebSocket 帧（透传 req_id）
    &stream_id,          // 流式消息 ID（使用 generate_req_id("stream") 生成）
    "回复内容",          // 回复内容（支持 Markdown）
    false,               // 是否结束流式消息
    None,                // 图文混排项（仅 finish=true 时有效）
    None,                // 反馈信息（仅首次回复时设置）
);
```

### `reply_welcome` 详细说明

发送欢迎语回复，需在收到 `event.enter_chat` 事件 5 秒内调用。

```rust
// 文本欢迎语
use serde_json::json;
await client.reply_welcome(&frame, json!({
    "msgtype": "text",
    "text": {"content": "欢迎！"},
}));

// 模板卡片欢迎语
await client.reply_welcome(&frame, json!({
    "msgtype": "template_card",
    "template_card": {"card_type": "text_notice", "main_title": {"title": "欢迎"}},
}));
```

### `send_message` 详细说明

主动向指定会话推送消息，无需依赖收到的回调帧。

```rust
// 发送 Markdown 消息
await client.send_message("userid_or_chatid", json!({
    "msgtype": "markdown",
    "markdown": {"content": "这是一条**主动推送**的消息"},
}));

// 发送模板卡片消息
await client.send_message("userid_or_chatid", json!({
    "msgtype": "template_card",
    "template_card": {"card_type": "text_notice", "main_title": {"title": "通知"}},
}));
```

### `download_file` 使用示例

```rust
// aes_key 取自消息体中的 image.aeskey 或 file.aeskey
client.on_message_image(|frame| {
    let client = client.clone();
    let frame = frame.clone();

    tokio::spawn(async move {
        let aes_key = frame
            .body
            .as_ref()
            .and_then(|v| v.as_object())
            .and_then(|b| b.get("image"))
            .and_then(|v| v.as_object())
            .and_then(|i| i.get("aeskey"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let url = frame
            .body
            .as_ref()
            .and_then(|v| v.as_object())
            .and_then(|b| b.get("image"))
            .and_then(|v| v.as_object())
            .and_then(|i| i.get("url"))
            .and_then(|v| v.as_str())
            .map(String::from);

        if let (Some(url), aes_key) = (url, aes_key) {
            let (buffer, filename) = client.download_file(&url, aes_key.as_deref()).await.unwrap();
            println!("文件名：{:?}, 大小：{} bytes", filename, buffer.len());
        }
    });
});
```

## 📋 消息类型

SDK 支持以下消息类型（`MessageType` 枚举）：

| 类型 | 值 | 说明 |
| --- | --- | --- |
| `MessageType::Text` | `"text"` | 文本消息 |
| `MessageType::Image` | `"image"` | 图片消息 |
| `MessageType::Mixed` | `"mixed"` | 图文混排消息 |
| `MessageType::Voice` | `"voice"` | 语音消息 |
| `MessageType::File` | `"file"` | 文件消息 |

SDK 支持以下事件类型（`EventType` 枚举）：

| 类型 | 值 | 说明 |
| --- | --- | --- |
| `EventType::EnterChat` | `"enter_chat"` | 进入会话事件 |
| `EventType::TemplateCardEvent` | `"template_card_event"` | 模板卡片事件 |
| `EventType::FeedbackEvent` | `"feedback_event"` | 用户反馈事件 |

## 🪵 自定义日志

实现 `Logger` Trait 接口即可自定义日志输出：

```rust
use wecom_aibot_rust_sdk::types::Logger;

struct MyLogger;

impl Logger for MyLogger {
    fn debug(&self, message: &str) {
        // 静默 debug 日志
    }

    fn info(&self, message: &str) {
        println!("[INFO] {}", message);
    }

    fn warn(&self, message: &str) {
        println!("[WARN] {}", message);
    }

    fn error(&self, message: &str) {
        println!("[ERROR] {}", message);
    }
}

let options = WSClientOptions {
    bot_id: "your-bot-id".to_string(),
    secret: "your-bot-secret".to_string(),
    logger: Some(Box::new(MyLogger)),
    ..Default::default()
};

let client = WSClient::new(options);
```

## 📂 项目结构

```
rust_wecom_bot_rust_sdk/
├── src/
│   ├── lib.rs             # 库入口文件，统一导出
│   ├── client.rs          # WSClient 核心客户端
│   ├── ws.rs              # WebSocket 长连接管理器
│   ├── message_handler.rs # 消息解析与事件分发
│   ├── api.rs             # HTTP API 客户端（文件下载）
│   ├── crypto_utils.rs    # AES-256-CBC 文件解密
│   ├── logger.rs          # 默认日志实现
│   ├── utils.rs           # 工具方法（generate_req_id 等）
│   └── types.rs           # 类型定义（枚举、结构体、常量）
├── examples/
│   └── basic.rs           # 基础使用示例
├── Cargo.toml             # 项目配置
├── README.md              # 本文件
└── .env.example           # 环境变量示例
```

## 🏗️ 架构说明

本 SDK 采用异步事件驱动架构，主要组件：

```
┌─────────────────────────────────────────────────────┐
│                     WSClient                        │
│  (核心客户端，提供事件注册和消息发送接口)            │
└─────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│  WsConnection   │  │ MessageHandler  │  │ WeComApiClient  │
│    Manager      │  │   (消息分发)    │  │   (文件下载)    │
│  (连接/心跳/重连) │  │                 │  │                 │
└─────────────────┘  └─────────────────┘  └─────────────────┘
         │                    │
         ▼                    ▼
┌─────────────────┐  ┌─────────────────┐
│  WebSocket LWS  │  │   Event Types   │
│   (长连接)      │  │  (文本/图片/事件)│
└─────────────────┘  └─────────────────┘
```

## 📄 License

MIT
