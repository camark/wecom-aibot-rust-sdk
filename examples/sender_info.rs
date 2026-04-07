/// 消息发送人信息示例 - 展示如何获取和显示消息发送人的详细信息

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

    // ========== 文本消息 - 显示发送人信息并回复 ==========

    let client_text = client.clone();
    client.on_message_text(move |frame| {
        let client_text = client_text.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            let body = frame.body.as_ref().and_then(|v| v.as_object());

            // 获取发送人信息 - 企业微信使用 from 字段
            let sender_info = body
                .and_then(|b| b.get("from"))
                .and_then(|v| v.as_object());

            let userid = sender_info
                .and_then(|s| s.get("userid"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let username = sender_info
                .and_then(|s| s.get("username"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let gender = sender_info
                .and_then(|s| s.get("gender"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // 获取消息内容
            let text_content = body
                .and_then(|b| b.get("text"))
                .and_then(|v| v.as_object())
                .and_then(|t| t.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // 显示完整的消息信息
            println!("\n========== 收到文本消息 ==========");
            println!("👤 发送人 ID: {}", userid);
            println!("📛 发送人姓名：{}", username);
            println!("⚧️ 性别：{}", gender);
            println!("💬 消息内容：{}", text_content);
            println!("================================\n");

            // 生成流式消息 ID
            let stream_id = generate_req_id("stream");

            // 构建包含发送人信息的回复
            let gender_emoji = match gender {
                "male" => "👨",
                "female" => "👩",
                _ => "👤",
            };

            let reply_content = format!(
                "收到您的消息啦！\n\n{}\n来自：{} ({})\n\n您说的是：\"{}\"",
                gender_emoji, username, userid, text_content
            );

            // 使用流式回复
            match client_text
                .reply_stream(&frame, &stream_id, &reply_content, true, None, None)
                .await
            {
                Ok(_) => {
                    println!("✅ 回复成功");
                },
                Err(e) => eprintln!("❌ 回复失败：{}", e),
            }
        });
    }).await;

    // ========== 图片消息 - 显示发送人信息 ==========

    let client_image = client.clone();
    client.on_message_image(move |frame| {
        let client_image = client_image.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            let body = frame.body.as_ref().and_then(|v| v.as_object());

            // 获取发送人信息
            let sender_info = body
                .and_then(|b| b.get("sender"))
                .and_then(|v| v.as_object());

            let userid = sender_info
                .and_then(|s| s.get("userid"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let username = sender_info
                .and_then(|s| s.get("username"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let image_url = body
                .and_then(|b| b.get("image"))
                .and_then(|v| v.as_object())
                .and_then(|i| i.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            println!("\n========== 收到图片消息 ==========");
            println!("👤 发送人 ID: {}", userid);
            println!("📛 发送人姓名：{}", username);
            println!("🖼️ 图片 URL: {}", image_url);
            println!("================================\n");

            let stream_id = generate_req_id("stream");
            let _ = client_image
                .reply_stream(
                    &frame,
                    &stream_id,
                    &format!("收到您的图片了，{}！🖼️", username),
                    true,
                    None,
                    None,
                )
                .await;
        });
    }).await;

    // ========== 文件消息 - 显示发送人信息 ==========

    let client_file = client.clone();
    client.on_message_file(move |frame| {
        let client_file = client_file.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            let body = frame.body.as_ref().and_then(|v| v.as_object());

            // 获取发送人信息
            let sender_info = body
                .and_then(|b| b.get("sender"))
                .and_then(|v| v.as_object());

            let userid = sender_info
                .and_then(|s| s.get("userid"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let username = sender_info
                .and_then(|s| s.get("username"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let file_url = body
                .and_then(|b| b.get("file"))
                .and_then(|v| v.as_object())
                .and_then(|f| f.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let file_name = body
                .and_then(|b| b.get("file"))
                .and_then(|v| v.as_object())
                .and_then(|f| f.get("filename"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            println!("\n========== 收到文件消息 ==========");
            println!("👤 发送人 ID: {}", userid);
            println!("📛 发送人姓名：{}", username);
            println!("📁 文件名：{}", file_name);
            println!("🔗 文件 URL: {}", file_url);
            println!("================================\n");

            let stream_id = generate_req_id("stream");
            let _ = client_file
                .reply_stream(
                    &frame,
                    &stream_id,
                    &format!("收到您的文件了，{}！📁\n文件名：{}", username, file_name),
                    true,
                    None,
                    None,
                )
                .await;
        });
    }).await;

    // ========== 事件回调 - 欢迎语（显示发送人信息） ==========

    let client_welcome = client.clone();
    client.on_event_enter_chat(move |frame| {
        let frame = frame.clone();
        let client_welcome = client_welcome.clone();

        tokio::spawn(async move {
            let body = frame.body.as_ref().and_then(|v| v.as_object());

            // 获取发送人信息
            let sender_info = body
                .and_then(|b| b.get("sender"))
                .and_then(|v| v.as_object());

            let userid = sender_info
                .and_then(|s| s.get("userid"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let username = sender_info
                .and_then(|s| s.get("username"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            println!("\n👋 用户进入会话：{} ({})", username, userid);

            let welcome_body = json!({
                "msgtype": "text",
                "text": {
                    "content": format!("您好，{}！👋 我是智能助手机器人，有什么可以帮您的吗？\n\n您可以：\n1. 发送文本消息 - 我会回复您\n2. 发送图片 - 我会确认收到\n3. 发送文件 - 我会显示文件信息", username)
                }
            });

            let _ = client_welcome.reply_welcome(&frame, welcome_body).await;
        });
    }).await;

    // ========== 启动 ==========

    println!("🤖 正在连接机器人...");
    println!("📊 本示例将显示消息发送人的详细信息\n");

    match client.connect().await {
        Ok(_) => {
            println!("✅ 机器人连接成功");
            println!("📱 请在企业微信中发送消息进行测试");
            println!("⏳ 监听消息中... (按 Ctrl+C 退出)");

            // 等待退出信号
            tokio::signal::ctrl_c().await.ok();

            println!("\n🛑 正在停止机器人...");
            client.disconnect();
        }
        Err(e) => {
            eprintln!("❌ 机器人连接失败：{}", e);
        }
    }
}
