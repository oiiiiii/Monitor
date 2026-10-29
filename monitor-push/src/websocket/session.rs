//! WebSocket 客户端会话
//!
//! 管理单个 WebSocket 连接的生命周期：
//! - 订阅/取消订阅床位
//! - 心跳检测
//! - 消息发送

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use super::message::{ClientAction, ClientMessage, PushMessage};

/// 客户端会话
pub struct ClientSession {
    /// 客户端 ID
    pub client_id: String,
    /// 订阅的床位号集合
    pub subscribed_beds: std::collections::HashSet<u16>,
    /// 消息发送器
    pub tx: mpsc::Sender<PushMessage>,
}

impl ClientSession {
    /// 创建新的客户端会话
    pub fn new(client_id: String, tx: mpsc::Sender<PushMessage>) -> Self {
        Self {
            client_id,
            subscribed_beds: std::collections::HashSet::new(),
            tx,
        }
    }

    /// 订阅床位
    pub fn subscribe(&mut self, bed_no: u16) -> bool {
        let is_new = self.subscribed_beds.insert(bed_no);
        if is_new {
            debug!(client = %self.client_id, bed = bed_no, "订阅床位");
        }
        is_new
    }

    /// 取消订阅
    pub fn unsubscribe(&mut self, bed_no: u16) -> bool {
        let existed = self.subscribed_beds.remove(&bed_no);
        if existed {
            debug!(client = %self.client_id, bed = bed_no, "取消订阅床位");
        }
        existed
    }

    /// 检查是否订阅了指定床位
    pub fn is_subscribed(&self, bed_no: u16) -> bool {
        self.subscribed_beds.contains(&bed_no)
    }

    /// 发送消息
    pub async fn send(&self, msg: PushMessage) {
        let _ = self.tx.send(msg).await;
    }
}

/// 管理所有客户端会话
#[derive(Clone)]
pub struct ClientManager {
    inner: Arc<Inner>,
}

struct Inner {
    /// 客户端集合（client_id -> 发送器）
    clients: std::sync::Mutex<std::collections::HashMap<String, mpsc::Sender<PushMessage>>>,
    /// 最大连接数
    max_connections: usize,
}

impl ClientManager {
    /// 创建新的客户端管理器
    pub fn new(max_connections: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                clients: std::sync::Mutex::new(std::collections::HashMap::new()),
                max_connections,
            }),
        }
    }

    /// 添加客户端
    pub fn add_client(&self, client_id: &str, tx: mpsc::Sender<PushMessage>) -> bool {
        let mut clients = self.inner.clients.lock().unwrap();

        if clients.len() >= self.inner.max_connections {
            warn!(
                client = client_id,
                current = clients.len(),
                max = self.inner.max_connections,
                "连接数已达上限，拒绝新连接"
            );
            return false;
        }

        clients.insert(client_id.to_string(), tx);
        info!(
            client = client_id,
            total = clients.len(),
            "客户端连接"
        );
        true
    }

    /// 移除客户端
    pub fn remove_client(&self, client_id: &str) {
        let mut clients = self.inner.clients.lock().unwrap();
        if clients.remove(client_id).is_some() {
            info!(
                client = client_id,
                total = clients.len(),
                "客户端断开"
            );
        }
    }

    /// 广播消息到订阅了指定床位的所有客户端
    pub fn broadcast_to_bed(&self, bed_no: u16, msg: PushMessage) {
        // 这里简化处理：广播给所有客户端，客户端自己过滤
        // 实际生产环境应该维护床位 -> 客户端的映射
        let clients = self.inner.clients.lock().unwrap();
        let clients_vec: Vec<_> = clients.values().cloned().collect();
        drop(clients);

        let msg = Arc::new(msg);
        for tx in clients_vec {
            let msg_clone = (*msg).clone();
            tokio::spawn(async move {
                let _ = tx.send(msg_clone).await;
            });
        }
    }

    /// 获取当前连接数
    pub fn connection_count(&self) -> usize {
        let clients = self.inner.clients.lock().unwrap();
        clients.len()
    }
}
