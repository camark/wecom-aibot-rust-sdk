/// WebSocket 长连接管理器
///
/// 对标 Node.js SDK src/ws.ts
/// 负责维护与企业微信的 WebSocket 长连接，包括心跳、重连、认证、串行回复队列等。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, WebSocketStream};
use tokio_tungstenite::MaybeTlsStream;

use crate::types::{Logger, SdkError, WsCmd, WsFrame, WsFrameHeaders, WSClientOptions};
use crate::utils::generate_req_id;

/// SDK 内置默认 WebSocket 连接地址
const DEFAULT_WS_URL: &str = "wss://openws.work.weixin.qq.com";

/// 回复队列中的单个任务项
struct ReplyQueueItem {
    frame: WsFrame,
    tx: oneshot::Sender<Result<WsFrame, SdkError>>,
}

/// WebSocket 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
}

/// WebSocket 长连接管理器
///
/// 负责维护与企业微信的 WebSocket 长连接，包括心跳、重连、认证等。
pub struct WsConnectionManager {
    logger: Arc<dyn Logger>,
    options: WSClientOptions,

    // 连接状态
    state: Arc<Mutex<ConnectionState>>,
    reconnect_attempts: Arc<AtomicUsize>,
    is_manual_close: Arc<Mutex<bool>>,

    // WebSocket 连接
    ws_tx: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,

    // 心跳相关
    missed_pong_count: Arc<AtomicUsize>,
    max_missed_pong: usize,

    // 串行回复队列
    reply_queues: Arc<Mutex<std::collections::HashMap<String, Vec<ReplyQueueItem>>>>,
    pending_acks: Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<WsFrame, SdkError>>>>>,
    processing_queues: Arc<Mutex<std::collections::HashSet<String>>>,

    // 事件接收器
    event_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<WsFrame>>>>,
}

impl WsConnectionManager {
    /// 创建新的 WebSocket 连接管理器
    pub fn new(options: WSClientOptions, logger: Arc<dyn Logger>) -> Self {
        let (_event_tx, event_rx) = mpsc::unbounded_channel::<WsFrame>();

        Self {
            logger,
            options: options.clone(),
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            reconnect_attempts: Arc::new(AtomicUsize::new(0)),
            is_manual_close: Arc::new(Mutex::new(false)),
            ws_tx: Arc::new(Mutex::new(None)),
            missed_pong_count: Arc::new(AtomicUsize::new(0)),
            max_missed_pong: 2,
            reply_queues: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pending_acks: Arc::new(Mutex::new(std::collections::HashMap::new())),
            processing_queues: Arc::new(Mutex::new(std::collections::HashSet::new())),
            event_rx: Arc::new(Mutex::new(Some(event_rx))),
        }
    }

    /// 获取事件接收器
    pub fn get_event_receiver(&self) -> mpsc::UnboundedReceiver<WsFrame> {
        self.event_rx.lock().take().expect("Event receiver already taken")
    }

    /// 建立 WebSocket 连接
    pub async fn connect(&self) -> Result<(), SdkError> {
        *self.is_manual_close.lock() = false;

        let ws_url = self.options.ws_url.as_deref().unwrap_or(DEFAULT_WS_URL);
        self.logger.info(&format!("Connecting to WebSocket: {}...", ws_url));

        match self._connect_impl(ws_url).await {
            Ok(_) => {
                self.logger.info("WebSocket connection established");
                Ok(())
            }
            Err(e) => {
                self.logger.error(&format!("Failed to create WebSocket connection: {}", e));
                Err(e)
            }
        }
    }

    async fn _connect_impl(&self, ws_url: &str) -> Result<(), SdkError> {
        let mut request = ws_url.into_client_request()?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            "aibot".parse().unwrap(),
        );

        let (ws_stream, _) = connect_async_tls_with_config(
            request,
            None,
            false,
            None,
        ).await?;

        let (mut ws_write, mut ws_read) = ws_stream.split();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Message>();

        *self.ws_tx.lock() = Some(msg_tx);
        *self.state.lock() = ConnectionState::Connected;
        self.reconnect_attempts.store(0, Ordering::SeqCst);
        self.missed_pong_count.store(0, Ordering::SeqCst);

        // 发送认证帧
        self._send_auth(&mut ws_write).await?;

        // 启动消息处理循环
        let ws_tx_clone = self.ws_tx.clone();
        let state_clone = self.state.clone();
        let logger_clone = self.logger.clone();
        let reply_queues_clone = self.reply_queues.clone();
        let pending_acks_clone = self.pending_acks.clone();
        let missed_pong_clone = self.missed_pong_count.clone();
        let max_missed_pong = self.max_missed_pong;
        let heartbeat_interval = self.options.heartbeat_interval;
        let reconnect_attempts = self.reconnect_attempts.clone();
        let is_manual_close = self.is_manual_close.clone();

        tokio::spawn(async move {
            Self::_receive_loop(
                &mut ws_read,
                &mut msg_rx,
                &ws_tx_clone,
                &state_clone,
                &logger_clone,
                &reply_queues_clone,
                &pending_acks_clone,
                &missed_pong_clone,
                max_missed_pong,
                heartbeat_interval,
                &reconnect_attempts,
                &is_manual_close,
            ).await;
        });

        Ok(())
    }

    async fn _receive_loop(
        ws_read: &mut futures::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        msg_rx: &mut mpsc::UnboundedReceiver<Message>,
        ws_tx: &Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
        state: &Arc<Mutex<ConnectionState>>,
        logger: &Arc<dyn Logger>,
        reply_queues: &Arc<Mutex<std::collections::HashMap<String, Vec<ReplyQueueItem>>>>,
        pending_acks: &Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<WsFrame, SdkError>>>>>,
        missed_pong_count: &Arc<AtomicUsize>,
        max_missed_pong: usize,
        heartbeat_interval: u64,
        _reconnect_attempts: &Arc<AtomicUsize>,
        _is_manual_close: &Arc<Mutex<bool>>,
    ) {
        let mut heartbeat_interval = time::interval(Duration::from_millis(heartbeat_interval));

        loop {
            tokio::select! {
                Some(msg_result) = ws_read.next() => {
                    match msg_result {
                        Ok(msg) => {
                            if let Message::Text(text) = msg {
                                if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                                    Self::_handle_frame(
                                        &frame,
                                        state,
                                        logger,
                                        reply_queues,
                                        pending_acks,
                                        missed_pong_count,
                                    ).await;
                                }
                            }
                        }
                        Err(e) => {
                            logger.error(&format!("WebSocket error: {}", e));
                            break;
                        }
                    }
                }
                Some(_msg) = msg_rx.recv() => {
                    // 发送消息已经在外部处理
                }
                _ = heartbeat_interval.tick() => {
                    // 心跳逻辑
                    let count = missed_pong_count.fetch_add(1, Ordering::SeqCst) + 1;
                    if count >= max_missed_pong {
                        logger.warn(&format!("No heartbeat ack received for {} consecutive pings, connection considered dead", count));
                        break;
                    }

                    let heartbeat_frame = WsFrame {
                        cmd: Some(WsCmd::HEARTBEAT.to_string()),
                        headers: WsFrameHeaders::new(generate_req_id(WsCmd::HEARTBEAT)),
                        body: None,
                        errcode: None,
                        errmsg: None,
                    };

                    if let Some(tx) = ws_tx.lock().as_ref() {
                        let _ = tx.send(Message::Text(serde_json::to_string(&heartbeat_frame).unwrap_or_default()));
                    }
                }
            }
        }
    }

    async fn _send_auth<W>(&self, ws_write: &mut W) -> Result<(), SdkError>
    where
        W: SinkExt<Message> + Unpin,
        W::Error: std::fmt::Display,
    {
        let auth_frame = WsFrame {
            cmd: Some(WsCmd::SUBSCRIBE.to_string()),
            headers: WsFrameHeaders::new(generate_req_id(WsCmd::SUBSCRIBE)),
            body: Some(json!({
                "bot_id": self.options.bot_id,
                "secret": self.options.secret
            })),
            errcode: None,
            errmsg: None,
        };

        let msg = Message::Text(serde_json::to_string(&auth_frame)?);
        ws_write.send(msg).await.map_err(|e| {
            SdkError::WebSocket(format!("Failed to send auth frame: {}", e))
        })?;

        self.logger.info("Auth frame sent");
        Ok(())
    }

    async fn _handle_frame(
        frame: &WsFrame,
        state: &Arc<Mutex<ConnectionState>>,
        logger: &Arc<dyn Logger>,
        reply_queues: &Arc<Mutex<std::collections::HashMap<String, Vec<ReplyQueueItem>>>>,
        pending_acks: &Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<WsFrame, SdkError>>>>>,
        missed_pong_count: &Arc<AtomicUsize>,
    ) {
        let cmd = frame.cmd.as_deref();

        // 消息推送
        if cmd == Some(WsCmd::CALLBACK) || cmd == Some(WsCmd::EVENT_CALLBACK) {
            logger.debug(&format!("Received push message: {:?}", frame.body));
            return;
        }

        // 无 cmd 的帧：认证响应、心跳响应或回复消息回执
        let req_id = frame.headers.req_id.as_str();

        // 检查是否是回复消息的回执
        if pending_acks.lock().contains_key(req_id) {
            Self::_handle_reply_ack(req_id, frame, pending_acks, reply_queues, logger).await;
            return;
        }

        if req_id.starts_with(WsCmd::SUBSCRIBE) {
            // 认证响应
            let errcode = frame.errcode.unwrap_or(-1);
            if errcode != 0 {
                logger.error(&format!(
                    "Authentication failed: errcode={}, errmsg={}",
                    errcode,
                    frame.errmsg.as_deref().unwrap_or("unknown")
                ));
                return;
            }
            logger.info("Authentication successful");
            *state.lock() = ConnectionState::Authenticated;
            missed_pong_count.store(0, Ordering::SeqCst);
            return;
        }

        if req_id.starts_with(WsCmd::HEARTBEAT) {
            // 心跳响应
            let errcode = frame.errcode.unwrap_or(-1);
            if errcode != 0 {
                logger.warn(&format!(
                    "Heartbeat ack error: errcode={}, errmsg={}",
                    errcode,
                    frame.errmsg.as_deref().unwrap_or("unknown")
                ));
                return;
            }
            missed_pong_count.store(0, Ordering::SeqCst);
            logger.debug("Received heartbeat ack");
            return;
        }

        // 未知帧类型
        logger.warn(&format!("Received unknown frame: {:?}", frame));
    }

    async fn _handle_reply_ack(
        req_id: &str,
        frame: &WsFrame,
        pending_acks: &Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<WsFrame, SdkError>>>>>,
        reply_queues: &Arc<Mutex<std::collections::HashMap<String, Vec<ReplyQueueItem>>>>,
        logger: &Arc<dyn Logger>,
    ) {
        let tx = pending_acks.lock().remove(req_id);
        if let Some(tx) = tx {
            let errcode = frame.errcode.unwrap_or(-1);
            if errcode != 0 {
                logger.warn(&format!(
                    "Reply ack error: reqId={}, errcode={}, errmsg={}",
                    req_id,
                    errcode,
                    frame.errmsg.as_deref().unwrap_or("unknown")
                ));
                let _ = tx.send(Err(SdkError::Internal(format!(
                    "Reply ack error: errcode={}, errmsg={}",
                    errcode,
                    frame.errmsg.as_deref().unwrap_or("unknown")
                ))));
            } else {
                logger.debug(&format!("Reply ack received for reqId: {}", req_id));
                let _ = tx.send(Ok(frame.clone()));
            }

            // 处理队列中的下一项
            let mut queues = reply_queues.lock();
            if let Some(queue) = queues.get_mut(req_id) {
                if !queue.is_empty() {
                    queue.remove(0);
                }
                if queue.is_empty() {
                    queues.remove(req_id);
                }
            }
        }
    }

    /// 发送数据帧
    pub async fn send(&self, frame: &WsFrame) -> Result<(), SdkError> {
        let ws_tx = self.ws_tx.lock();
        if let Some(tx) = ws_tx.as_ref() {
            let msg = Message::Text(serde_json::to_string(frame)?);
            tx.send(msg).map_err(|e| {
                SdkError::WebSocket(format!("Failed to send message: {}", e))
            })?;
            Ok(())
        } else {
            Err(SdkError::WebSocket("WebSocket not connected".to_string()))
        }
    }

    /// 通过 WebSocket 通道发送回复消息（串行队列版本）
    pub async fn send_reply(
        &self,
        req_id: &str,
        body: serde_json::Value,
        cmd: &str,
    ) -> Result<WsFrame, SdkError> {
        let (tx, rx) = oneshot::channel();

        let frame = WsFrame {
            cmd: Some(cmd.to_string()),
            headers: WsFrameHeaders::new(req_id.to_string()),
            body: Some(body),
            errcode: None,
            errmsg: None,
        };

        let item = ReplyQueueItem { frame, tx };

        let mut queues = self.reply_queues.lock();
        let queue = queues.entry(req_id.to_string()).or_insert_with(Vec::new);

        if queue.len() >= 100 {
            return Err(SdkError::Internal(format!(
                "Reply queue for reqId {} exceeds max size (100)",
                req_id
            )));
        }

        queue.push(item);

        // 如果队列中只有这一条，立即开始处理
        if queue.len() == 1 {
            let req_id_clone = req_id.to_string();
            let self_clone = self.clone_for_task();
            tokio::spawn(async move {
                self_clone._process_reply_queue(&req_id_clone).await;
            });
        }

        match rx.await {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(SdkError::Timeout("Reply ack timeout".to_string())),
        }
    }

    fn clone_for_task(&self) -> Self {
        Self {
            logger: self.logger.clone(),
            options: self.options.clone(),
            state: self.state.clone(),
            reconnect_attempts: self.reconnect_attempts.clone(),
            is_manual_close: self.is_manual_close.clone(),
            ws_tx: self.ws_tx.clone(),
            missed_pong_count: self.missed_pong_count.clone(),
            max_missed_pong: self.max_missed_pong,
            reply_queues: self.reply_queues.clone(),
            pending_acks: self.pending_acks.clone(),
            processing_queues: self.processing_queues.clone(),
            event_rx: Arc::new(Mutex::new(None)),
        }
    }

    async fn _process_reply_queue(&self, req_id: &str) {
        self.processing_queues.lock().insert(req_id.to_string());

        loop {
            // 检查队列长度
            let queue_len = {
                let queues = self.reply_queues.lock();
                queues.get(req_id).map(|q| q.len()).unwrap_or(0)
            };

            if queue_len == 0 {
                self.reply_queues.lock().remove(req_id);
                break;
            }

            // 获取队首元素
            let item_opt = {
                let queues = self.reply_queues.lock();
                queues.get(req_id).and_then(|q| q.first().map(|item| item.frame.clone()))
            };

            let frame = match item_opt {
                Some(f) => f,
                None => {
                    self.reply_queues.lock().remove(req_id);
                    break;
                }
            };

            // 发送消息
            match self.send(&frame).await {
                Ok(_) => {
                    self.logger.debug(&format!(
                        "Reply message sent via WebSocket, reqId: {}",
                        req_id
                    ));
                }
                Err(e) => {
                    self.logger.error(&format!("Failed to send reply for reqId {}: {}", req_id, e));
                    self.reply_queues.lock().remove(req_id);
                    break;
                }
            }

            // 设置超时
            let (ack_tx, ack_rx) = oneshot::channel();
            self.pending_acks.lock().insert(req_id.to_string(), ack_tx);

            let timeout = time::sleep(Duration::from_secs(5));
            tokio::pin!(timeout);

            tokio::select! {
                result = ack_rx => {
                    match result {
                        Ok(Ok(_frame)) => {
                            // 成功收到回执
                            self.logger.debug(&format!("Reply ack received for reqId: {}", req_id));
                        }
                        Ok(Err(e)) => {
                            self.logger.warn(&format!("Reply ack error for reqId {}: {}", req_id, e));
                        }
                        Err(_) => {
                            self.logger.warn(&format!("Reply ack timeout for reqId: {}", req_id));
                        }
                    }
                }
                _ = &mut timeout => {
                    self.logger.warn(&format!("Reply ack timeout (5s) for reqId: {}", req_id));
                }
            }

            // 移除已处理的项目
            {
                let mut queues = self.reply_queues.lock();
                if let Some(queue) = queues.get_mut(req_id) {
                    if !queue.is_empty() {
                        queue.remove(0);
                    }
                }
            }

            if queue_len <= 1 {
                self.reply_queues.lock().remove(req_id);
                break;
            }
        }

        self.processing_queues.lock().remove(req_id);
    }

    /// 主动断开连接
    pub fn disconnect(&self) {
        *self.is_manual_close.lock() = true;
        self.logger.info("WebSocket connection manually closed");
    }

    /// 获取当前连接状态
    pub fn is_connected(&self) -> bool {
        *self.state.lock() == ConnectionState::Authenticated
    }
}
