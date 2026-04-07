/// 文件和图片下载测试 - 测试下载图片和文件功能

use std::env;
use std::path::{Path, PathBuf};

use dotenv::dotenv;
use serde_json::json;
use wecom_aibot_rust_sdk::{generate_req_id, WSClient, WSClientOptions};

/// 获取下载目录
/// 优先使用环境变量 WECHAT_BOT_DOWNLOAD_DIR，否则使用系统 temp 目录
fn get_download_dir() -> PathBuf {
    env::var("WECHAT_BOT_DOWNLOAD_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir())
}

#[tokio::main]
async fn main() {
    // 加载 .env 文件中的环境变量
    if Path::new(".env").exists() {
        dotenv().ok();
    }

    let bot_id = env::var("WECHAT_BOT_ID").expect("WECHAT_BOT_ID not set");
    let secret = env::var("WECHAT_BOT_SECRET").expect("WECHAT_BOT_SECRET not set");

    // 获取下载目录
    let download_dir = get_download_dir();
    println!("📂 下载文件存放目录：{}", download_dir.display());

    // 确保下载目录存在
    if let Err(e) = std::fs::create_dir_all(&download_dir) {
        eprintln!("⚠️ 创建下载目录失败：{}", e);
    }

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

    // ========== 文本消息 - 简单回复 ==========

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

        let frame = frame.clone();
        tokio::spawn(async move {
            let stream_id = generate_req_id("stream");
            match client_text
                .reply_stream(&frame, &stream_id, &text_content, true, None, None)
                .await
            {
                Ok(reply_frame) => {
                    println!("✅ 回复成功，回执：cmd={:?}, errcode={:?}",
                        reply_frame.cmd, reply_frame.errcode);
                },
                Err(e) => eprintln!("❌ 回复失败：{}", e),
            }
        });
    }).await;

    // ========== 图片消息 - 下载并解密 ==========

    let client_image = client.clone();
    let download_dir_image = download_dir.clone();
    client.on_message_image(move |frame| {
        let client_image = client_image.clone();
        let download_dir_image = download_dir_image.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            let body = frame.body.as_ref().and_then(|v| v.as_object());
            let image_url = body
                .and_then(|b| b.get("image"))
                .and_then(|v| v.as_object())
                .and_then(|i| i.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let aes_key = body
                .and_then(|b| b.get("image"))
                .and_then(|v| v.as_object())
                .and_then(|i| i.get("aeskey"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            println!("🖼️ 收到图片消息：{}", image_url);
            println!("   AES Key: {}", aes_key);
            let stream_id = generate_req_id("stream");

            // 先回复收到图片的提示
            let _ = client_image
                .reply_stream(
                    &frame,
                    &stream_id,
                    "收到您的图片消息 🖼️，正在下载...",
                    false,
                    None,
                    None,
                )
                .await;

            // 下载并解密图片
            match client_image.download_file(&image_url, Some(&aes_key)).await {
                Ok((data, filename)) => {
                    println!("   ✅ 图片下载成功，大小：{} bytes, 文件名：{:?}", data.len(), filename);

                    // 回复下载成功
                    let _ = client_image
                        .reply_stream(
                            &frame,
                            &stream_id,
                            &format!("✅ 图片下载成功！\n大小：{} bytes", data.len()),
                            true,
                            None,
                            None,
                        )
                        .await;

                    // 保存文件到本地
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let output_path = download_dir_image.join(format!("downloaded_image_{}.jpg", timestamp));
                    if let Err(e) = std::fs::write(&output_path, &data) {
                        eprintln!("   ⚠️ 保存文件失败：{}", e);
                    } else {
                        println!("   ✅ 文件已保存到：{}", output_path.display());
                    }
                }
                Err(e) => {
                    eprintln!("   ❌ 图片下载失败：{}", e);
                    let _ = client_image
                        .reply_stream(
                            &frame,
                            &stream_id,
                            &format!("❌ 图片下载失败：{}", e),
                            true,
                            None,
                            None,
                        )
                        .await;
                }
            }
        });
    }).await;

    // ========== 文件消息 - 下载并解密 ==========

    let client_file = client.clone();
    let download_dir_file = download_dir.clone();
    client.on_message_file(move |frame| {
        let client_file = client_file.clone();
        let download_dir_file = download_dir_file.clone();
        let frame = frame.clone();

        tokio::spawn(async move {
            let body = frame.body.as_ref().and_then(|v| v.as_object());
            let file_url = body
                .and_then(|b| b.get("file"))
                .and_then(|v| v.as_object())
                .and_then(|f| f.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let aes_key = body
                .and_then(|b| b.get("file"))
                .and_then(|v| v.as_object())
                .and_then(|f| f.get("aeskey"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let file_name = body
                .and_then(|b| b.get("file"))
                .and_then(|v| v.as_object())
                .and_then(|f| f.get("filename"))
                .and_then(|v| v.as_str())
                .unwrap_or("downloaded_file");

            println!("📁 收到文件消息：{}", file_url);
            println!("   文件名：{}", file_name);
            println!("   AES Key: {}", aes_key);
            let stream_id = generate_req_id("stream");

            // 先回复收到文件的提示
            let _ = client_file
                .reply_stream(
                    &frame,
                    &stream_id,
                    &format!("收到您的文件消息 📁\n文件名：{}\n正在下载...", file_name),
                    false,
                    None,
                    None,
                )
                .await;

            // 下载并解密文件
            match client_file.download_file(&file_url, Some(&aes_key)).await {
                Ok((data, filename)) => {
                    println!("   ✅ 文件下载成功，大小：{} bytes, 文件名：{:?}", data.len(), filename);

                    // 回复下载成功
                    let _ = client_file
                        .reply_stream(
                            &frame,
                            &stream_id,
                            &format!("✅ 文件下载成功！\n文件名：{}\n大小：{} bytes", file_name, data.len()),
                            true,
                            None,
                            None,
                        )
                        .await;

                    // 保存文件到本地
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let output_path = download_dir_file.join(format!("downloaded_file_{}_{}",
                        timestamp,
                        filename.as_deref().unwrap_or("unknown")
                    ));
                    if let Err(e) = std::fs::write(&output_path, &data) {
                        eprintln!("   ⚠️ 保存文件失败：{}", e);
                    } else {
                        println!("   ✅ 文件已保存到：{}", output_path.display());
                    }
                }
                Err(e) => {
                    eprintln!("   ❌ 文件下载失败：{}", e);
                    let _ = client_file
                        .reply_stream(
                            &frame,
                            &stream_id,
                            &format!("❌ 文件下载失败：{}", e),
                            true,
                            None,
                            None,
                        )
                        .await;
                }
            }
        });
    }).await;

    // ========== 事件回调 - 欢迎语 ==========

    let client_welcome = client.clone();
    client.on_event_enter_chat(move |frame| {
        let frame = frame.clone();
        let client_welcome = client_welcome.clone();

        tokio::spawn(async move {
            println!("👋 用户进入会话，发送欢迎语");

            let welcome_body = json!({
                "msgtype": "text",
                "text": {
                    "content": "您好！我是文件下载测试机器人 🤖\n\n支持的测试：\n1. 发送文本消息 - 我会原样回复\n2. 发送图片 - 我会下载并保存\n3. 发送文件 - 我会下载并保存\n\n下载的文件会保存在当前目录。"
                }
            });

            let _ = client_welcome.reply_welcome(&frame, welcome_body).await;
        });
    }).await;

    // ========== 启动 ==========

    println!("🤖 正在连接文件下载测试机器人...");

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
