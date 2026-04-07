/// WebSocket 长连接管理器
///
/// 对标 Node.js SDK src/ws.ts
/// 负责维护与企业微信的 WebSocket 长连接，包括心跳、重连、认证、串行回复队列等。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::Mutex;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, WebSocketStream};
use tokio_tungstenite::MaybeTlsStream;

use crate::types::{Logger, SdkError, WsCmd, WsFrame, WsFrameHeaders, WSClientOptions};
use crate::utils::generate_req_id;

/// SDK 内置默认 WebSocket 连接地址
const DEFAULT_WS_URL: &str = "wss://openws.work.weixin.qq.com";

/// WebSocket 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
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

    // 事件通道
    event_tx: mpsc::UnboundedSender<WsFrame>,
    event_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<WsFrame>>>>,
}

impl WsConnectionManager {
    /// 创建新的 WebSocket 连接管理器
    pub fn new(options: WSClientOptions, logger: Arc<dyn Logger>) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel::<WsFrame>();

        Self {
            logger,
            options: options.clone(),
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            reconnect_attempts: Arc::new(AtomicUsize::new(0)),
            is_manual_close: Arc::new(Mutex::new(false)),
            ws_tx: Arc::new(Mutex::new(None)),
            missed_pong_count: Arc::new(AtomicUsize::new(0)),
            max_missed_pong: 2,
            event_tx,
            event_rx: Arc::new(Mutex::new(Some(event_rx))),
        }
    }

    /// 获取事件接收器
    pub async fn get_event_receiver(&self) -> mpsc::UnboundedReceiver<WsFrame> {
        self.event_rx.lock().await.take().expect("Event receiver already taken")
    }

    /// 建立 WebSocket 连接
    pub async fn connect(&self) -> Result<(), SdkError> {
        *self.is_manual_close.lock().await = false;

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

        let (ws_write, mut ws_read) = ws_stream.split();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Message>();

        *self.ws_tx.lock().await = Some(msg_tx);
        *self.state.lock().await = ConnectionState::Connected;
        self.reconnect_attempts.store(0, Ordering::SeqCst);
        self.missed_pong_count.store(0, Ordering::SeqCst);

        // 发送认证帧
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
        let auth_msg = Message::Text(serde_json::to_string(&auth_frame)?);

        // 使用 msg_tx 发送认证帧
        if let Some(tx) = self.ws_tx.lock().await.as_ref() {
            let _ = tx.send(auth_msg);
        }

        // 启动消息处理循环
        let state_clone = self.state.clone();
        let logger_clone = self.logger.clone();
        let missed_pong_clone = self.missed_pong_count.clone();
        let max_missed_pong = self.max_missed_pong;
        let heartbeat_interval = self.options.heartbeat_interval;
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            Self::_receive_loop(
                &mut ws_read,
                ws_write,
                &mut msg_rx,
                &state_clone,
                &logger_clone,
                &missed_pong_clone,
                max_missed_pong,
                heartbeat_interval,
                &event_tx,
            ).await;
        });

        self.logger.info("Auth frame sent");
        Ok(())
    }

    async fn _receive_loop(
        ws_read: &mut futures::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        mut ws_write: futures::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        msg_rx: &mut mpsc::UnboundedReceiver<Message>,
        state: &Arc<Mutex<ConnectionState>>,
        logger: &Arc<dyn Logger>,
        missed_pong_count: &Arc<AtomicUsize>,
        max_missed_pong: usize,
        heartbeat_interval: u64,
        event_tx: &mpsc::UnboundedSender<WsFrame>,
    ) {
        let mut heartbeat_interval = time::interval(Duration::from_millis(heartbeat_interval));

        loop {
            tokio::select! {
                Some(msg_result) = ws_read.next() => {
                    match msg_result {
                        Ok(msg) => {
                            if let Message::Text(text) = msg {
                                logger.debug(&format!("Received raw WebSocket message: {}", text));
                                if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                                    Self::_handle_frame(
                                        &frame,
                                        state,
                                        logger,
                                        missed_pong_count,
                                        event_tx,
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
                Some(msg) = msg_rx.recv() => {
                    // 通过 WebSocket 实际发送消息
                    if ws_write.send(msg).await.is_err() {
                        logger.error("Failed to send message via WebSocket");
                        break;
                    }
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

                    if ws_write.send(Message::Text(serde_json::to_string(&heartbeat_frame).unwrap_or_default())).await.is_err() {
                        logger.error("Failed to send heartbeat via WebSocket");
                        break;
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
        missed_pong_count: &Arc<AtomicUsize>,
        event_tx: &mpsc::UnboundedSender<WsFrame>,
    ) {
        let cmd = frame.cmd.as_deref();
        let req_id = frame.headers.req_id.as_str();

        // 消息推送回调 - 转发到事件通道
        if cmd == Some(WsCmd::CALLBACK) || cmd == Some(WsCmd::EVENT_CALLBACK) {
            logger.debug(&format!("Received push message: {:?}, req_id={}", frame.body, req_id));
            let _ = event_tx.send(frame.clone());
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
            *state.lock().await = ConnectionState::Authenticated;
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
        logger.debug(&format!("Received frame: cmd={:?}, req_id={}, errcode={:?}", cmd, req_id, frame.errcode));
    }

    /// 发送数据帧
    pub async fn send(&self, frame: &WsFrame) -> Result<(), SdkError> {
        let ws_tx = self.ws_tx.lock().await;
        let tx_clone = ws_tx.clone();
        drop(ws_tx); // 释放锁后再发送消息

        if let Some(tx) = tx_clone.as_ref() {
            let msg_str = serde_json::to_string(frame)?;
            self.logger.debug(&format!("Sending message: {}", msg_str));
            match tx.send(Message::Text(msg_str)) {
                Ok(_) => {
                    self.logger.debug("Message sent successfully on wire");
                    Ok(())
                },
                Err(e) => {
                    self.logger.error(&format!("Failed to send message on wire: {}", e));
                    Err(SdkError::WebSocket(format!("Failed to send message: {}", e)))
                }
            }
        } else {
            Err(SdkError::WebSocket("WebSocket not connected".to_string()))
        }
    }

    /// 通过 WebSocket 通道发送回复消息
    ///
    /// 注意：企业微信服务器对于 aibot_respond_msg 命令可能不返回 ack，
    /// 因此我们在发送成功后直接返回，不等待 ack。
    pub async fn send_reply(
        &self,
        req_id: &str,
        body: serde_json::Value,
        cmd: &str,
    ) -> Result<WsFrame, SdkError> {
        let frame = WsFrame {
            cmd: Some(cmd.to_string()),
            headers: WsFrameHeaders::new(req_id.to_string()),
            body: Some(body),
            errcode: None,
            errmsg: None,
        };

        // 直接发送消息
        self.send(&frame).await?;

        self.logger.debug(&format!("Reply message sent, reqId: {}", req_id));

        // 返回成功回执（模拟的，服务器可能不会返回 ack）
        Ok(WsFrame {
            cmd: Some(cmd.to_string()),
            headers: WsFrameHeaders::new(req_id.to_string()),
            body: None,
            errcode: Some(0),
            errmsg: None,
        })
    }

    /// 主动断开连接
    pub fn disconnect(&self) {
        *self.is_manual_close.blocking_lock() = true;
        self.logger.info("WebSocket connection manually closed");
    }

    /// 获取当前连接状态
    pub fn is_connected(&self) -> bool {
        *self.state.blocking_lock() == ConnectionState::Authenticated
    }
}
