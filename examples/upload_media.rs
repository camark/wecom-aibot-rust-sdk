/// 上传临时素材示例 - 演示如何上传文件到企业微信

use std::env;
use std::path::Path;
use std::fs;

use dotenv::dotenv;
use wecom_aibot_rust_sdk::{WSClient, WSClientOptions};

#[tokio::main]
async fn main() {
    // 加载 .env 文件中的环境变量
    if Path::new(".env").exists() {
        dotenv().ok();
    }

    let bot_id = env::var("WECHAT_BOT_ID").expect("WECHAT_BOT_ID not set");
    let secret = env::var("WECHAT_BOT_SECRET").expect("WECHAT_BOT_SECRET not set");

    // 检查是否提供了文件路径参数
    let file_path = env::args().nth(1).expect("用法：cargo run --example upload_media <文件路径>");

    // 读取文件
    let file_data = fs::read(&file_path).expect("无法读取文件");
    let filename = Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    // 根据文件扩展名判断媒体类型
    let media_type = match Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg" | "png" | "gif") => "image",
        Some("amr") => "voice",
        Some("mp4") => "video",
        _ => "file",
    };

    println!("📁 准备上传文件：{}", filename);
    println!("📊 文件类型：{}", media_type);
    println!("📏 文件大小：{} bytes", file_data.len());

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

    client.on_error(|error| {
        eprintln!("❌ 发生错误：{}", error);
    }).await;

    // ========== 连接并上传 ==========

    println!("🔌 正在连接...");
    match client.connect().await {
        Ok(_) => {
            println!("✅ 连接成功");

            // 等待认证成功
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // 上传文件
            println!("📤 正在上传文件...");
            match client.upload_media(media_type, &file_data, filename).await {
                Ok(result) => {
                    println!("✅ 上传成功！");
                    println!("📋 上传结果：{:#?}", result);

                    // 提取 media_id
                    if let Some(media_id) = result.get("media_id").and_then(|v| v.as_str()) {
                        println!("🏷️ media_id: {}", media_id);
                        println!("\n💡 提示：可以使用此 media_id 发送图片/文件消息");
                    }
                }
                Err(e) => {
                    eprintln!("❌ 上传失败：{}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ 连接失败：{}", e);
        }
    }

    println!("\n🛑 正在停止...");
    client.disconnect();
}
