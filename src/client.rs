/// WSClient 核心客户端
///
/// 对标 Node.js SDK src/client.ts
/// 提供企业微信智能机器人的核心功能

use std::sync::Arc;

use serde_json::json;
use tokio::sync::{Mutex, RwLock};
use tokio::sync::mpsc;

use crate::api::WeComApiClient;
use crate::crypto_utils::decrypt_file;
use crate::logger::DefaultLogger;
use crate::message_handler::{MessageEvent, MessageHandler};
use crate::types::{
    Logger, SdkError, WsCmd, WsFrame, WSClientOptions,
};
use crate::utils::generate_req_id;
use crate::ws::WsConnectionManager;

/// 企业微信智能机器人 Rust SDK 核心客户端
///
/// 基于 tokio 异步运行时，提供 WebSocket 长连接消息收发能力。
#[derive(Clone)]
pub struct WSClient {
    options: WSClientOptions,
    logger: Arc<dyn Logger>,
    started: Arc<RwLock<bool>>,

    // API 客户端（仅用于文件下载）
    api_client: Arc<WeComApiClient>,

    // WebSocket 管理器
    ws_manager: Arc<WsConnectionManager>,

    // 消息处理器
    message_handler: Arc<MessageHandler>,

    // 事件处理器存储
    event_handlers: Arc<Mutex<EventHandlers>>,
}

/// 事件处理器集合
#[derive(Clone)]
struct EventHandlers {
    connected: Vec<Arc<dyn Fn() + Send + Sync>>,
    authenticated: Vec<Arc<dyn Fn() + Send + Sync>>,
    disconnected: Vec<Arc<dyn Fn(&str) + Send + Sync>>,
    reconnecting: Vec<Arc<dyn Fn(usize) + Send + Sync>>,
    error: Vec<Arc<dyn Fn(&SdkError) + Send + Sync>>,
    message: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    message_text: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    message_image: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    message_mixed: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    message_voice: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    message_file: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    event: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    event_enter_chat: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    event_template_card: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
    event_feedback: Vec<Arc<dyn Fn(&WsFrame) + Send + Sync>>,
}

impl Default for EventHandlers {
    fn default() -> Self {
        Self {
            connected: Vec::new(),
            authenticated: Vec::new(),
            disconnected: Vec::new(),
            reconnecting: Vec::new(),
            error: Vec::new(),
            message: Vec::new(),
            message_text: Vec::new(),
            message_image: Vec::new(),
            message_mixed: Vec::new(),
            message_voice: Vec::new(),
            message_file: Vec::new(),
            event: Vec::new(),
            event_enter_chat: Vec::new(),
            event_template_card: Vec::new(),
            event_feedback: Vec::new(),
        }
    }
}

impl WSClient {
    /// 创建新的客户端实例
    pub fn new(options: WSClientOptions) -> Self {
        let logger: Arc<dyn Logger> = options
            .logger
            .clone()
            .unwrap_or_else(|| Arc::new(DefaultLogger::default()));

        let request_timeout = options.request_timeout;
        let logger_clone = logger.clone();

        // 初始化 API 客户端（仅用于文件下载）
        let api_client = Arc::new(WeComApiClient::new(logger_clone, request_timeout));

        // 初始化 WebSocket 管理器
        let ws_manager = Arc::new(WsConnectionManager::new(options.clone(), logger.clone()));

        // 初始化消息处理器
        let message_handler = Arc::new(MessageHandler::new(logger.clone()));

        Self {
            options,
            logger,
            started: Arc::new(RwLock::new(false)),
            api_client,
            ws_manager,
            message_handler,
            event_handlers: Arc::new(Mutex::new(EventHandlers::default())),
        }
    }

    /// 建立 WebSocket 长连接
    ///
    /// SDK 使用内置默认地址建立连接，连接成功后自动发送认证帧（bot_id + secret）。
    ///
    /// # Returns
    /// 返回 Ok(()) 表示连接成功
    pub async fn connect(&self) -> Result<(), SdkError> {
        let mut started = self.started.write().await;
        if *started {
            self.logger.warn("Client already connected");
            return Ok(());
        }

        self.logger.info("Establishing WebSocket connection...");
        *started = true;

        self.ws_manager.connect().await?;

        // 启动接收循环，处理消息回调
        self._start_receive_loop();

        Ok(())
    }

    /// 启动接收循环，处理收到的消息并调用用户注册的回调
    fn _start_receive_loop(&self) {
        let ws_manager = self.ws_manager.clone();
        let event_handlers = self.event_handlers.clone();
        let message_handler = self.message_handler.clone();
        let logger = self.logger.clone();

        tokio::spawn(async move {
            let event_rx = ws_manager.get_event_receiver().await;
            Self::_receive_loop(event_rx, event_handlers, message_handler, logger).await;
        });
    }

    async fn _receive_loop(
        mut event_rx: mpsc::UnboundedReceiver<WsFrame>,
        event_handlers: Arc<Mutex<EventHandlers>>,
        message_handler: Arc<MessageHandler>,
        _logger: Arc<dyn Logger>,
    ) {
        while let Some(frame) = event_rx.recv().await {
            // 解析帧并获取事件列表
            let events = message_handler.handle_frame(&frame);

            // 调用对应的回调
            for event in events {
                let handlers = event_handlers.lock().await;
                match event {
                    MessageEvent::Message(f) => {
                        for handler in &handlers.message {
                            handler(&f);
                        }
                    }
                    MessageEvent::Text(f) => {
                        for handler in &handlers.message_text {
                            handler(&f);
                        }
                    }
                    MessageEvent::Image(f) => {
                        for handler in &handlers.message_image {
                            handler(&f);
                        }
                    }
                    MessageEvent::Mixed(f) => {
                        for handler in &handlers.message_mixed {
                            handler(&f);
                        }
                    }
                    MessageEvent::Voice(f) => {
                        for handler in &handlers.message_voice {
                            handler(&f);
                        }
                    }
                    MessageEvent::File(f) => {
                        for handler in &handlers.message_file {
                            handler(&f);
                        }
                    }
                    MessageEvent::Event(f) => {
                        for handler in &handlers.event {
                            handler(&f);
                        }
                    }
                    MessageEvent::EnterChat(f) => {
                        for handler in &handlers.event_enter_chat {
                            handler(&f);
                        }
                    }
                    MessageEvent::TemplateCardEvent(f) => {
                        for handler in &handlers.event_template_card {
                            handler(&f);
                        }
                    }
                    MessageEvent::FeedbackEvent(f) => {
                        for handler in &handlers.event_feedback {
                            handler(&f);
                        }
                    }
                }
            }
        }
    }

    /// 断开 WebSocket 连接
    pub fn disconnect(&self) {
        self.ws_manager.disconnect();
        self.logger.info("Disconnected");
    }

    /// 通过 WebSocket 通道发送回复消息（通用方法）
    ///
    /// # Arguments
    /// * `frame` - 收到的原始 WebSocket 帧，透传 headers.req_id
    /// * `body` - 回复消息体
    /// * `cmd` - 发送的命令类型
    ///
    /// # Returns
    /// 回执帧
    pub async fn reply(
        &self,
        frame: &WsFrame,
        body: serde_json::Value,
        cmd: Option<&str>,
    ) -> Result<WsFrame, SdkError> {
        let req_id = &frame.headers.req_id;
        let cmd = cmd.unwrap_or(WsCmd::RESPONSE);
        self.ws_manager.send_reply(req_id, body, cmd).await
    }

    /// 发送流式文本回复（便捷方法）
    ///
    /// # Arguments
    /// * `frame` - 收到的原始 WebSocket 帧，透传 headers.req_id
    /// * `stream_id` - 流式消息 ID
    /// * `content` - 回复内容（支持 Markdown）
    /// * `finish` - 是否结束流式消息，默认 false
    /// * `msg_item` - 图文混排项（仅在 finish=true 时有效）
    /// * `feedback` - 反馈信息（仅在首次回复时设置）
    ///
    /// # Returns
    /// 回执帧
    pub async fn reply_stream(
        &self,
        frame: &WsFrame,
        stream_id: &str,
        content: &str,
        finish: bool,
        msg_item: Option<Vec<serde_json::Value>>,
        feedback: Option<serde_json::Value>,
    ) -> Result<WsFrame, SdkError> {
        let mut stream_data = json!({
            "id": stream_id,
            "finish": finish,
            "content": content,
        });

        // msg_item 仅在 finish=true 时支持
        if finish {
            if let Some(items) = msg_item {
                if !items.is_empty() {
                    stream_data["msg_item"] = json!(items);
                }
            }
        }

        // feedback 仅在首次回复时设置
        if let Some(fb) = feedback {
            stream_data["feedback"] = fb;
        }

        self.reply(
            frame,
            json!({
                "msgtype": "stream",
                "stream": stream_data,
            }),
            None,
        )
        .await
    }

    /// 发送欢迎语回复
    ///
    /// 注意：此方法需要使用对应事件（如 enter_chat）的 req_id 才能调用。
    /// 收到事件回调后需在 5 秒内发送回复，超时将无法发送欢迎语。
    ///
    /// # Arguments
    /// * `frame` - 对应事件的 WebSocket 帧
    /// * `body` - 欢迎语消息体（支持文本或模板卡片格式）
    ///
    /// # Returns
    /// 回执帧
    pub async fn reply_welcome(
        &self,
        frame: &WsFrame,
        body: serde_json::Value,
    ) -> Result<WsFrame, SdkError> {
        self.reply(frame, body, Some(WsCmd::RESPONSE_WELCOME)).await
    }

    /// 回复模板卡片消息
    ///
    /// # Arguments
    /// * `frame` - 收到的原始 WebSocket 帧
    /// * `template_card` - 模板卡片内容
    /// * `feedback` - 反馈信息
    ///
    /// # Returns
    /// 回执帧
    pub async fn reply_template_card(
        &self,
        frame: &WsFrame,
        template_card: serde_json::Value,
        feedback: Option<serde_json::Value>,
    ) -> Result<WsFrame, SdkError> {
        let card = if let Some(fb) = feedback {
            let mut card_obj = template_card.as_object().cloned().unwrap_or_default();
            card_obj.insert("feedback".to_string(), fb);
            json!(card_obj)
        } else {
            template_card
        };

        let body = json!({
            "msgtype": "template_card",
            "template_card": card,
        });

        self.reply(frame, body, None).await
    }

    /// 发送流式消息 + 模板卡片组合回复
    ///
    /// # Arguments
    /// * `frame` - 收到的原始 WebSocket 帧
    /// * `stream_id` - 流式消息 ID
    /// * `content` - 回复内容（支持 Markdown）
    /// * `finish` - 是否结束流式消息，默认 false
    /// * `msg_item` - 图文混排项（仅在 finish=true 时有效）
    /// * `stream_feedback` - 流式消息反馈信息（首次回复时设置）
    /// * `template_card` - 模板卡片内容（同一消息只能回复一次）
    /// * `card_feedback` - 模板卡片反馈信息
    ///
    /// # Returns
    /// 回执帧
    pub async fn reply_stream_with_card(
        &self,
        frame: &WsFrame,
        stream_id: &str,
        content: &str,
        finish: bool,
        msg_item: Option<Vec<serde_json::Value>>,
        stream_feedback: Option<serde_json::Value>,
        template_card: Option<serde_json::Value>,
        card_feedback: Option<serde_json::Value>,
    ) -> Result<WsFrame, SdkError> {
        let mut stream_data = json!({
            "id": stream_id,
            "finish": finish,
            "content": content,
        });

        if finish {
            if let Some(items) = msg_item {
                if !items.is_empty() {
                    stream_data["msg_item"] = json!(items);
                }
            }
        }

        if let Some(fb) = stream_feedback {
            stream_data["feedback"] = fb;
        }

        let mut body = json!({
            "msgtype": "stream_with_template_card",
            "stream": stream_data,
        });

        if let Some(card) = template_card {
            let card_obj = if let Some(fb) = card_feedback {
                let mut obj = card.as_object().cloned().unwrap_or_default();
                obj.insert("feedback".to_string(), fb);
                obj
            } else {
                card.as_object().cloned().unwrap_or_default()
            };
            body["template_card"] = json!(card_obj);
        }

        self.reply(frame, body, None).await
    }

    /// 更新模板卡片
    ///
    /// 注意：此方法需要使用对应事件（template_card_event）的 req_id 才能调用。
    /// 收到事件回调后需在 5 秒内发送回复，超时将无法更新卡片。
    ///
    /// # Arguments
    /// * `frame` - 对应事件的 WebSocket 帧
    /// * `template_card` - 模板卡片内容（task_id 需跟回调收到的 task_id 一致）
    /// * `userids` - 要替换模版卡片消息的 userid 列表
    ///
    /// # Returns
    /// 回执帧
    pub async fn update_template_card(
        &self,
        frame: &WsFrame,
        template_card: serde_json::Value,
        userids: Option<Vec<String>>,
    ) -> Result<WsFrame, SdkError> {
        let mut body = json!({
            "response_type": "update_template_card",
            "template_card": template_card,
        });

        if let Some(ids) = userids {
            if !ids.is_empty() {
                body["userids"] = json!(ids);
            }
        }

        self.reply(frame, body, Some(WsCmd::RESPONSE_UPDATE)).await
    }

    /// 主动发送消息
    ///
    /// 向指定会话（单聊或群聊）主动推送消息，无需依赖收到的回调帧。
    ///
    /// # Arguments
    /// * `chatid` - 会话 ID，单聊填用户的 userid，群聊填对应群聊的 chatid
    /// * `body` - 消息体（支持 markdown 或 template_card 格式）
    ///
    /// # Returns
    /// 回执帧
    pub async fn send_message(
        &self,
        chatid: &str,
        body: serde_json::Value,
    ) -> Result<WsFrame, SdkError> {
        let req_id = generate_req_id(WsCmd::SEND_MSG);
        let mut full_body = body.as_object().cloned().unwrap_or_default();
        full_body.insert("chatid".to_string(), json!(chatid));

        self.ws_manager
            .send_reply(&req_id, json!(full_body), WsCmd::SEND_MSG)
            .await
    }

    /// 下载文件并使用 AES 密钥解密
    ///
    /// # Arguments
    /// * `url` - 文件下载地址
    /// * `aes_key` - AES 解密密钥（Base64 编码），取自消息中 image.aeskey 或 file.aeskey
    ///
    /// # Returns
    /// (解密后的文件数据，文件名)
    pub async fn download_file(
        &self,
        url: &str,
        aes_key: Option<&str>,
    ) -> Result<(Vec<u8>, Option<String>), SdkError> {
        self.logger.info("Downloading and decrypting file...");

        // 下载加密的文件数据
        let (encrypted_data, filename) = self.api_client.download_file_raw(url).await?;

        // 如果没有提供 aes_key，直接返回原始数据
        if aes_key.is_none() {
            self.logger.warn("No aes_key provided, returning raw file data");
            return Ok((encrypted_data, filename));
        }

        // 使用 AES-256-CBC 解密
        let decrypted_data = decrypt_file(&encrypted_data, aes_key.unwrap())?;

        self.logger.info("File downloaded and decrypted successfully");
        Ok((decrypted_data, filename))
    }

    /// 获取当前连接状态
    pub fn is_connected(&self) -> bool {
        self.ws_manager.is_connected()
    }

    /// 获取 API 客户端实例（供高级用途使用）
    pub fn api(&self) -> Arc<WeComApiClient> {
        self.api_client.clone()
    }

    /// 注册事件处理器
    pub async fn on_connected<F>(&self, handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.event_handlers.lock().await.connected.push(Arc::new(handler));
    }

    pub async fn on_authenticated<F>(&self, handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .authenticated
            .push(Arc::new(handler));
    }

    pub async fn on_disconnected<F>(&self, handler: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .disconnected
            .push(Arc::new(handler));
    }

    pub async fn on_reconnecting<F>(&self, handler: F)
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .reconnecting
            .push(Arc::new(handler));
    }

    pub async fn on_error<F>(&self, handler: F)
    where
        F: Fn(&SdkError) + Send + Sync + 'static,
    {
        self.event_handlers.lock().await.error.push(Arc::new(handler));
    }

    pub async fn on_message<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers.lock().await.message.push(Arc::new(handler));
    }

    pub async fn on_message_text<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .message_text
            .push(Arc::new(handler));
    }

    pub async fn on_message_image<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .message_image
            .push(Arc::new(handler));
    }

    pub async fn on_message_mixed<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .message_mixed
            .push(Arc::new(handler));
    }

    pub async fn on_message_voice<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .message_voice
            .push(Arc::new(handler));
    }

    pub async fn on_message_file<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .message_file
            .push(Arc::new(handler));
    }

    pub async fn on_event<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers.lock().await.event.push(Arc::new(handler));
    }

    pub async fn on_event_enter_chat<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .event_enter_chat
            .push(Arc::new(handler));
    }

    pub async fn on_event_template_card<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .event_template_card
            .push(Arc::new(handler));
    }

    pub async fn on_event_feedback<F>(&self, handler: F)
    where
        F: Fn(&WsFrame) + Send + Sync + 'static,
    {
        self.event_handlers
            .lock()
            .await
            .event_feedback
            .push(Arc::new(handler));
    }
}
