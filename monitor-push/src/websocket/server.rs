//! WebSocket 服务器
//!
//! 监听 WebSocket 连接，处理客户端订阅，
//! 并将实时数据推送给订阅的客户端。

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{Error, Message};
use tracing::{debug, error, info, warn};

use super::message::{ClientAction, ClientMessage, PushMessage};
use super::session::{ClientManager, ClientSession};

/// WebSocket 服务器
pub struct WebSocketServer {
    listen_addr: String,
    client_manager: ClientManager,
    heartbeat_interval: Duration,
}

impl WebSocketServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(listen_addr: String, max_connections: usize, heartbeat_interval_secs: u64) -> Self {
        Self {
            listen_addr,
            client_manager: ClientManager::new(max_connections),
            heartbeat_interval: Duration::from_secs(heartbeat_interval_secs),
        }
    }

    /// 获取客户端管理器引用
    pub fn client_manager(&self) -> &ClientManager {
        &self.client_manager
    }

    /// 启动服务器
    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!(addr = %self.listen_addr, "WebSocket 服务器启动，监听中...");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let client_id = format!("{}", addr);
                    info!(client = %client_id, "新的 WebSocket 连接");

                    let client_manager = self.client_manager.clone();
                    let heartbeat_interval = self.heartbeat_interval;

                    tokio::spawn(async move {
                        Self::handle_connection(
                            stream,
                            client_id,
                            client_manager,
                            heartbeat_interval,
                        )
                        .await;
                    });
                }
                Err(e) => {
                    error!(error = %e, "接受 WebSocket 连接失败");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// 处理单个 WebSocket 连接
    async fn handle_connection(
        stream: TcpStream,
        client_id: String,
        client_manager: ClientManager,
        heartbeat_interval: Duration,
    ) {
        // 升级为 WebSocket
        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!(client = %client_id, error = %e, "WebSocket 握手失败");
                return;
            }
        };

        let (mut write, mut read) = ws_stream.split();

        // 创建消息通道（用于从其他任务发送消息到这个客户端）
        let (tx, mut rx) = mpsc::channel::<PushMessage>(1000);

        // 注册客户端
        if !client_manager.add_client(&client_id, tx.clone()) {
            // 连接数已满，发送错误消息并关闭
            let err_msg = PushMessage::error(0, "连接数已达上限");
            let _ = write.send(Message::Text(serde_json::to_string(&err_msg).unwrap_or_default())).await;
            let _ = write.close().await;
            return;
        }

        // 创建会话
        let mut session = ClientSession::new(client_id.clone(), tx);

        // 心跳定时器
        let mut heartbeat_tick = tokio::time::interval(heartbeat_interval);

        loop {
            tokio::select! {
                // 处理来自客户端的消息
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            Self::handle_client_message(&mut session, &text).await;
                        }
                        Some(Ok(Message::Binary(data))) => {
                            // 二进制消息，暂时忽略
                            debug!(client = %client_id, len = data.len(), "收到二进制消息");
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!(client = %client_id, "客户端主动关闭连接");
                            break;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            // 响应 Pong
                            let _ = write.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            // 收到 Pong，重置心跳计时器
                        }
                        Some(Ok(Message::Frame(_))) => {
                            // 原始帧，忽略
                        }
                        Some(Err(e)) => {
                            warn!(client = %client_id, error = %e, "WebSocket 读取错误");
                            break;
                        }
                        None => {
                            info!(client = %client_id, "WebSocket 连接关闭");
                            break;
                        }
                    }
                }

                // 处理需要发送给客户端的消息
                Some(msg) = rx.recv() => {
                    let json = match serde_json::to_string(&msg) {
                        Ok(j) => j,
                        Err(e) => {
                            error!(error = %e, "序列化推送消息失败");
                            continue;
                        }
                    };

                    if let Err(e) = write.send(Message::Text(json)).await {
                        warn!(client = %client_id, error = %e, "发送消息失败");
                        break;
                    }
                }

                // 心跳定时
                _ = heartbeat_tick.tick() => {
                    // 发送心跳
                    let heartbeat = PushMessage::heartbeat();
                    let json = serde_json::to_string(&heartbeat).unwrap_or_default();
                    if let Err(e) = write.send(Message::Text(json)).await {
                        warn!(client = %client_id, error = %e, "发送心跳失败");
                        break;
                    }
                }
            }
        }

        // 清理
        client_manager.remove_client(&client_id);
    }

    /// 处理客户端消息
    async fn handle_client_message(session: &mut ClientSession, text: &str) {
        // 解析客户端消息
        match serde_json::from_str::<ClientMessage>(text) {
            Ok(msg) => match msg.action {
                ClientAction::Subscribe => {
                    session.subscribe(msg.bed_no);
                    // 发送确认
                    let ack = PushMessage::subscribe_ack(msg.bed_no, true);
                    let _ = session.send(ack).await;
                }
                ClientAction::Unsubscribe => {
                    session.unsubscribe(msg.bed_no);
                }
                ClientAction::Ping => {
                    // 心跳响应已经在协议层面处理了
                }
                ClientAction::RequestHistory => {
                    // 历史数据请求，后续实现
                }
            },
            Err(e) => {
                warn!(client = %session.client_id, error = %e, "解析客户端消息失败");
                let err = PushMessage::error(0, "消息格式错误");
                let _ = session.send(err).await;
            }
        }
    }
}
