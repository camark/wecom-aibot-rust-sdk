/// 消息处理器
///
/// 对标 Node.js SDK src/message-handler.ts
/// 负责解析 WebSocket 帧并分发为具体的消息事件和事件回调。

use std::sync::Arc;

use crate::types::{WsCmd, WsFrame};
use crate::types::Logger;

/// 消息事件类型
#[derive(Debug, Clone)]
pub enum MessageEvent {
    /// 通用消息事件
    Message(WsFrame),
    /// 文本消息
    Text(WsFrame),
    /// 图片消息
    Image(WsFrame),
    /// 图文混排消息
    Mixed(WsFrame),
    /// 语音消息
    Voice(WsFrame),
    /// 文件消息
    File(WsFrame),
    /// 通用事件回调
    Event(WsFrame),
    /// 进入会话事件
    EnterChat(WsFrame),
    /// 模板卡片事件
    TemplateCardEvent(WsFrame),
    /// 用户反馈事件
    FeedbackEvent(WsFrame),
}

/// 消息处理器
///
/// 负责解析 WebSocket 帧并分发为具体的消息事件和事件回调。
pub struct MessageHandler {
    logger: Arc<dyn Logger>,
}

impl MessageHandler {
    pub fn new(logger: Arc<dyn Logger>) -> Self {
        Self { logger }
    }

    /// 处理收到的 WebSocket 帧，解析并触发对应的消息/事件
    ///
    /// # Arguments
    /// * `frame` - WebSocket 接收帧
    pub fn handle_frame(&self, frame: &WsFrame) -> Vec<MessageEvent> {
        match self.parse_frame(frame) {
            Ok(events) => events,
            Err(e) => {
                self.logger.error(&format!("Failed to handle message: {}", e));
                vec![]
            }
        }
    }

    /// 解析帧并返回事件列表
    fn parse_frame(&self, frame: &WsFrame) -> Result<Vec<MessageEvent>, String> {
        // 事件推送回调处理
        if frame.cmd.as_deref() == Some(WsCmd::EVENT_CALLBACK) {
            return Ok(self.handle_event_callback(frame));
        }

        // 消息推送回调处理
        Ok(self.handle_message_callback(frame))
    }

    /// 处理消息推送回调 (aibot_msg_callback)
    fn handle_message_callback(&self, frame: &WsFrame) -> Vec<MessageEvent> {
        let mut events = Vec::new();

        // 触发通用消息事件
        events.push(MessageEvent::Message(frame.clone()));

        // 根据 body 中的消息类型触发特定事件
        let body = frame.body.as_ref().map(|v| v.as_object()).flatten();
        let msgtype = body
            .and_then(|b| b.get("msgtype"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match msgtype {
            "text" => events.push(MessageEvent::Text(frame.clone())),
            "image" => events.push(MessageEvent::Image(frame.clone())),
            "mixed" => events.push(MessageEvent::Mixed(frame.clone())),
            "voice" => events.push(MessageEvent::Voice(frame.clone())),
            "file" => events.push(MessageEvent::File(frame.clone())),
            _ => {
                self.logger
                    .debug(&format!("Received unhandled message type: {}", msgtype));
            }
        }

        events
    }

    /// 处理事件推送回调 (aibot_event_callback)
    fn handle_event_callback(&self, frame: &WsFrame) -> Vec<MessageEvent> {
        let mut events = Vec::new();

        // 触发通用事件
        events.push(MessageEvent::Event(frame.clone()));

        // 根据事件类型触发特定事件
        let body = frame.body.as_ref().and_then(|v| v.as_object());
        let event_type = body
            .and_then(|b| b.get("event"))
            .and_then(|v| v.as_object())
            .and_then(|e| e.get("eventtype"))
            .and_then(|v| v.as_str());

        if let Some(event_type) = event_type {
            match event_type {
                "enter_chat" => events.push(MessageEvent::EnterChat(frame.clone())),
                "template_card_event" => {
                    events.push(MessageEvent::TemplateCardEvent(frame.clone()))
                }
                "feedback_event" => events.push(MessageEvent::FeedbackEvent(frame.clone())),
                _ => {
                    self.logger.debug(&format!(
                        "Received unknown event type: {}",
                        event_type
                    ));
                }
            }
        } else {
            self.logger.debug(&format!(
                "Received event callback without eventtype: {:?}",
                body
            ));
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::DefaultLogger;
    use serde_json::json;

    #[test]
    fn test_handle_text_message() {
        let logger = Box::new(DefaultLogger::default());
        let handler = MessageHandler::new(logger);

        let frame = WsFrame {
            cmd: Some(WsCmd::CALLBACK.to_string()),
            headers: crate::types::WsFrameHeaders::new("test_req_id"),
            body: Some(json!({
                "msgtype": "text",
                "text": {"content": "Hello"}
            })),
            errcode: None,
            errmsg: None,
        };

        let events = handler.handle_frame(&frame);

        // 应该收到两个事件：Message 和 Text
        assert!(matches!(events[0], MessageEvent::Message(_)));
        assert!(matches!(events[1], MessageEvent::Text(_)));
    }

    #[test]
    fn test_handle_event_callback() {
        let logger = Box::new(DefaultLogger::default());
        let handler = MessageHandler::new(logger);

        let frame = WsFrame {
            cmd: Some(WsCmd::EVENT_CALLBACK.to_string()),
            headers: crate::types::WsFrameHeaders::new("test_req_id"),
            body: Some(json!({
                "event": {"eventtype": "enter_chat"}
            })),
            errcode: None,
            errmsg: None,
        };

        let events = handler.handle_frame(&frame);

        // 应该收到两个事件：Event 和 EnterChat
        assert!(matches!(events[0], MessageEvent::Event(_)));
        assert!(matches!(events[1], MessageEvent::EnterChat(_)));
    }
}
