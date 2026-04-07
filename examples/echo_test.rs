/// Echo 测试 - 收到消息后自动回复相同内容

use std::env;
use std::path::Path;

use dotenv::dotenv;
use serde_json::json;
use wecom_aibot_rust_sdk::{generate_req_id, WSClient, WSClientOptions};

#[tokio::main]
async fn main() {
    // 加载 .env 文件中的环境变量
    if Path::new(".env").exists() {
        dotenv().ok();
    }

    let bot_id = env::var("WECHAT_BOT_ID").expect("WECHAT_BOT_ID not set");
    let secret = env::var("WECHAT_BOT_SECRET").expect("WECHAT_BOT_SECRET not set");

    // 创建 WSClient 实例
    let client = WSClient::new(WSClientOptions::new(&bot_id, &secret));

    // ========== 连接事件 ==========

    client.on_connected(|| {
        println!("✅ WebSocket 已连接");
    }).await;

    client.on_authenticated(|| {
        println!("✅ 认证成功");
    }).await;

    client.on_disconnected(|reason| {
        println!("❌ 连接已断开：{}", reason);
    }).await;

    client.on_reconnecting(|attempt| {
        println!("🔄 正在进行第 {} 次重连...", attempt);
    }).await;

    client.on_error(|error| {
        eprintln!("❌ 发生错误：{}", error);
    }).await;

    // ========== 消息事件 - Echo 回复 ==========

    let client_text = client.clone();
    client.on_message_text(move |frame| {
        let client_text = client_text.clone();

        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let text_content = body
            .and_then(|b| b.get("text"))
            .and_then(|v| v.as_object())
            .and_then(|t| t.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        println!("📩 收到文本消息：{}", text_content);

        // 克隆 frame 用于异步任务
        let frame = frame.clone();

        // 启动异步任务进行回复
        tokio::spawn(async move {
            // 生成流式消息 ID
            let stream_id = generate_req_id("stream");

            println!("📤 正在回复：{}", text_content);

            // 使用流式回复
            match client_text
                .reply_stream(&frame, &stream_id, &text_content, true, None, None)
                .await
            {
                Ok(_) => println!("✅ 回复成功"),
                Err(e) => eprintln!("❌ 回复失败：{}", e),
            }
        });
    }).await;

    // 也处理其他消息类型
    let client_image = client.clone();
    client.on_message_image(move |frame| {
        let client_image = client_image.clone();

        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let image_url = body
            .and_then(|b| b.get("image"))
            .and_then(|v| v.as_object())
            .and_then(|i| i.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        println!("🖼️ 收到图片消息：{}", image_url);

        // 克隆 frame 用于异步任务
        let frame = frame.clone();

        // 回复收到图片的提示
        tokio::spawn(async move {
            let stream_id = generate_req_id("stream");
            let _ = client_image
                .reply_stream(
                    &frame,
                    &stream_id,
                    "收到您的图片消息 🖼️",
                    true,
                    None,
                    None,
                )
                .await;
        });
    }).await;

    let client_file = client.clone();
    client.on_message_file(move |frame| {
        let client_file = client_file.clone();

        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let file_url = body
            .and_then(|b| b.get("file"))
            .and_then(|v| v.as_object())
            .and_then(|f| f.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        println!("📁 收到文件消息：{}", file_url);

        // 克隆 frame 用于异步任务
        let frame = frame.clone();

        // 回复收到文件的提示
        tokio::spawn(async move {
            let stream_id = generate_req_id("stream");
            let _ = client_file
                .reply_stream(
                    &frame,
                    &stream_id,
                    "收到您的文件消息 📁",
                    true,
                    None,
                    None,
                )
                .await;
        });
    }).await;

    // ========== 事件回调 ==========

    let client_welcome = client.clone();
    client.on_event_enter_chat(move |frame| {
        let frame = frame.clone();
        let client_welcome = client_welcome.clone();

        tokio::spawn(async move {
            println!("👋 用户进入会话，发送欢迎语");

            let welcome_body = json!({
                "msgtype": "text",
                "text": {
                    "content": "您好！我是 Echo 机器人，您发送的消息我会原样回复！\n\n支持的测试：\n1. 发送文本消息 - 我会原样回复\n2. 发送图片 - 我会回复'收到您的图片消息'\n3. 发送文件 - 我会回复'收到您的文件消息'"
                }
            });

            let _ = client_welcome.reply_welcome(&frame, welcome_body).await;
        });
    }).await;

    // ========== 启动 ==========

    println!("🤖 正在连接 Echo 机器人...");

    match client.connect().await {
        Ok(_) => {
            println!("✅ Echo 机器人连接成功");
            println!("📱 请在企业微信中发送消息进行测试");
            println!("⏳ 监听消息中... (按 Ctrl+C 退出)");

            // 等待退出信号
            tokio::signal::ctrl_c().await.ok();

            println!("\n🛑 正在停止 Echo 机器人...");
            client.disconnect();
        }
        Err(e) => {
            eprintln!("❌ 机器人连接失败：{}", e);
        }
    }
}
