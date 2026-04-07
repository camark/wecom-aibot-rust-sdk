/// 企业微信智能机器人 SDK 基本使用示例
///
/// 对标 Node.js SDK examples/basic.ts

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

    // 创建 WSClient 实例
    let client = WSClient::new(WSClientOptions::new(
        env::var("WECHAT_BOT_ID").unwrap_or_default(),
        env::var("WECHAT_BOT_SECRET").unwrap_or_default(),
    ));

    // 模板卡片示例数据
    let _template_card = json!({
        "card_type": "multiple_interaction",
        "source": {
            "icon_url": "https://wework.qpic.cn/wwpic/252813_jOfDHtcISzuodLa_1629280209/0",
            "desc": "企业微信"
        },
        "main_title": {
            "title": "欢迎使用企业微信",
            "desc": "您的好友正在邀请您加入企业微信"
        },
        "select_list": [
            {
                "question_key": "question_key_one",
                "title": "选择标签 1",
                "disable": false,
                "selected_id": "id_one",
                "option_list": [
                    {"id": "id_one", "text": "选择器选项 1"},
                    {"id": "id_two", "text": "选择器选项 2"}
                ]
            },
            {
                "question_key": "question_key_two",
                "title": "选择标签 2",
                "selected_id": "id_three",
                "option_list": [
                    {"id": "id_three", "text": "选择器选项 3"},
                    {"id": "id_four", "text": "选择器选项 4"}
                ]
            }
        ],
        "submit_button": {"text": "提交", "key": "submit_key"},
        "task_id": "task_id_123456"
    });

    // ========== 连接事件 ==========

    client.on_connected(|| {
        println!("WebSocket 已连接");
    });

    client.on_authenticated(|| {
        println!("认证成功");
    });

    client.on_disconnected(|reason| {
        println!("连接已断开：{}", reason);
    });

    client.on_reconnecting(|attempt| {
        println!("正在进行第 {} 次重连...", attempt);
    });

    client.on_error(|error| {
        eprintln!("发生错误：{}", error);
    });

    // ========== 消息事件 ==========

    client.on_message(|frame| {
        let body_str = serde_json::to_string_pretty(&frame.body).unwrap_or_default();
        println!("收到消息：{}", body_str.chars().take(200).collect::<String>());
    });

    let _client_text = client.clone();
    client.on_message_text(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let text_content = body
            .and_then(|b| b.get("text"))
            .and_then(|v| v.as_object())
            .and_then(|t| t.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("收到文本消息：{}", text_content);

        // 注意：实际使用中需要在异步上下文中调用 reply_stream
        // 这里仅做演示
        let _stream_id = generate_req_id("stream");
    });

    client.on_message_image(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let image_url = body
            .and_then(|b| b.get("image"))
            .and_then(|v| v.as_object())
            .and_then(|i| i.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("收到图片消息：{}", image_url);
    });

    client.on_message_mixed(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let items = body
            .and_then(|b| b.get("mixed"))
            .and_then(|v| v.as_object())
            .and_then(|m| m.get("msg_item"))
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        println!("收到图文混排消息，包含 {} 个子项", items);
    });

    client.on_message_voice(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let voice_content = body
            .and_then(|b| b.get("voice"))
            .and_then(|v| v.as_object())
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("收到语音消息（转文本）：{}", voice_content);
    });

    client.on_message_file(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let file_url = body
            .and_then(|b| b.get("file"))
            .and_then(|v| v.as_object())
            .and_then(|f| f.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("收到文件消息：{}", file_url);
    });

    // ========== 事件回调 ==========

    client.on_event_enter_chat(|frame| {
        println!("用户进入会话");
        // 注意：实际使用需要在异步上下文中调用 reply_welcome
        let _frame = frame;
    });

    client.on_event_template_card(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let event = body
            .and_then(|b| b.get("event"))
            .and_then(|v| v.as_object());

        let event_key = event
            .and_then(|e| e.get("event_key"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("收到模板卡片事件：{}", event_key);
    });

    client.on_event_feedback(|frame| {
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let event = body.and_then(|b| b.get("event"));

        println!(
            "收到用户反馈事件：{}",
            serde_json::to_string_pretty(&event).unwrap_or_default()
        );
    });

    // ========== 启动 ==========

    println!("正在启动机器人...");

    match client.connect().await {
        Ok(_) => {
            println!("机器人启动成功，按 Ctrl+C 退出");

            // 等待退出信号
            tokio::signal::ctrl_c().await.ok();

            println!("\n正在停止机器人...");
            client.disconnect();
        }
        Err(e) => {
            eprintln!("机器人启动失败：{}", e);
        }
    }
}
