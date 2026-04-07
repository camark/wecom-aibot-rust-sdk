/// 发送测试消息示例

use std::env;
use std::path::Path;

use dotenv::dotenv;
use serde_json::json;
use tokio::time::{self, Duration};
use wecom_aibot_rust_sdk::{WSClient, WSClientOptions};

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

    client.on_error(|error| {
        eprintln!("❌ 发生错误：{}", error);
    }).await;

    // ========== 消息事件 ==========

    client.on_message_text(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let text_content = body
            .and_then(|b| b.get("text"))
            .and_then(|v| v.as_object())
            .and_then(|t| t.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("📩 收到文本消息：{}", text_content);
    }).await;

    client.on_message_image(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let image_url = body
            .and_then(|b| b.get("image"))
            .and_then(|v| v.as_object())
            .and_then(|i| i.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("🖼️ 收到图片消息：{}", image_url);
    }).await;

    client.on_event_enter_chat(|frame| {
        println!("👋 用户进入会话");
        let _frame = frame;
    }).await;

    // ========== 启动 ==========

    println!("🤖 正在连接机器人...");

    match client.connect().await {
        Ok(_) => {
            println!("✅ 机器人连接成功");

            // 等待 2 秒确保连接稳定
            time::sleep(Duration::from_secs(2)).await;

            // ========== 发送测试消息 ==========

            // 注意：send_message 需要 chatid，这通常来自收到的消息回调
            // 这里演示如何构建消息体
            println!("\n📤 演示消息发送功能：");

            // 1. Markdown 消息示例
            let markdown_body = json!({
                "msgtype": "markdown",
                "markdown": {
                    "content": "**测试消息**\n\n这是一条来自 Rust SDK 的测试消息\n- 时间：2024\n- 状态：✅ 正常"
                }
            });
            println!("   Markdown 消息体：{}", markdown_body);

            // 2. 文本消息示例
            let text_body = json!({
                "msgtype": "text",
                "text": {
                    "content": "您好！这是一条测试消息。"
                }
            });
            println!("   文本消息体：{}", text_body);

            // 3. 模板卡片消息示例
            let card_body = json!({
                "msgtype": "template_card",
                "template_card": {
                    "card_type": "text_notice",
                    "source": {
                        "icon_url": "https://wework.qpic.cn/wwpic/252813_jOfDHtcISzuodLa_1629280209/0",
                        "desc": "企业微信"
                    },
                    "main_title": {
                        "title": "测试通知",
                        "desc": "这是一条模板卡片测试消息"
                    }
                }
            });
            println!("   模板卡片消息体：{}", card_body);

            println!("\n💡 提示：要实际发送消息，需要在收到消息回调后调用:");
            println!("   - reply(frame, body, None) - 回复消息");
            println!("   - reply_stream(frame, stream_id, content, finish, None, None) - 流式回复");
            println!("   - send_message(chatid, body) - 主动发送消息 (需要 chatid)");

            println!("\n⏳ 监听消息中... (按 Ctrl+C 退出)");

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
