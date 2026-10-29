//! WebSocket 推送服务（单机版）

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use monitor_common::{
    metrics::health_server::HealthProvider,
    metrics::{ComponentHealth, HealthCheck, HealthStatus, PushMetrics},
    types::{AlarmEvent, VitalSignsData, WaveformData},
};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{
        broadcast,
        mpsc::{self, Sender},
        Mutex,
    },
};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};
use tracing::{debug, error, info, warn};

use crate::bus::SharedMessageBus;

type ClientId = String;

/// 客户端消息
#[derive(Debug, Deserialize)]
struct ClientMessage {
    action: String,
    bed_no: Option<u16>,
}

/// 推送消息类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PushType {
    VitalSigns,
    Waveform,
    Alarm,
    DeviceStatus,
    Heartbeat,
    SubscribeAck,
    Error,
}

/// 推送消息
#[derive(Debug, Serialize)]
struct PushMessage {
    msg_type: PushType,
    bed_no: u16,
    timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// 客户端管理器
pub struct ClientManager {
    clients: HashMap<ClientId, Sender<Message>>,
    subscriptions: HashMap<u16, HashSet<ClientId>>,
}

impl ClientManager {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
            subscriptions: HashMap::new(),
        }
    }

    fn add_client(&mut self, client_id: &str, tx: Sender<Message>) -> bool {
        self.clients.insert(client_id.to_string(), tx);
        true
    }

    fn remove_client(&mut self, client_id: &str) {
        self.clients.remove(client_id);
        for beds in self.subscriptions.values_mut() {
            beds.remove(client_id);
        }
    }

    fn subscribe(&mut self, client_id: &str, bed_no: u16) {
        self.subscriptions
            .entry(bed_no)
            .or_insert_with(HashSet::new)
            .insert(client_id.to_string());
    }

    fn unsubscribe(&mut self, client_id: &str, bed_no: u16) {
        if let Some(beds) = self.subscriptions.get_mut(&bed_no) {
            beds.remove(client_id);
        }
    }

    async fn broadcast_to_bed(&self, bed_no: u16, msg: &str) -> usize {
        let mut sent = 0;
        if let Some(clients) = self.subscriptions.get(&bed_no) {
            for client_id in clients {
                if let Some(tx) = self.clients.get(client_id) {
                    if tx.send(Message::Text(msg.to_string())).await.is_ok() {
                        sent += 1;
                    }
                }
            }
        }
        sent
    }

    fn client_count(&self) -> usize {
        self.clients.len()
    }
}

/// WebSocket 服务器
pub struct WsPushServer {
    listen_addr: String,
    bus: SharedMessageBus,
    manager: Arc<Mutex<ClientManager>>,
    metrics: Arc<PushMetrics>,
    heartbeat_interval: u64,
}

impl WsPushServer {
    pub fn new(
        listen_addr: String,
        bus: SharedMessageBus,
        metrics: Arc<PushMetrics>,
        heartbeat_interval: u64,
    ) -> Self {
        Self {
            listen_addr,
            bus,
            manager: Arc::new(Mutex::new(ClientManager::new())),
            metrics,
            heartbeat_interval,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!("WebSocket 推送服务已启动: {}", self.listen_addr);

        // 启动体征广播任务
        self.spawn_vital_broadcaster();
        // 启动波形广播任务
        self.spawn_waveform_broadcaster();
        // 启动报警广播任务
        self.spawn_alarm_broadcaster();

        // 接受连接
        while let Ok((stream, addr)) = listener.accept().await {
            let manager = self.manager.clone();
            let metrics = self.metrics.clone();
            let heartbeat = self.heartbeat_interval;

            tokio::spawn(async move {
                info!(client = %addr, "WebSocket 连接请求");

                let callback = |req: &Request, mut response: Response| {
                    debug!("WebSocket path: {}", req.uri().path());
                    let headers = response.headers_mut();
                    headers.insert("Server", "monitor-standalone".parse().unwrap());
                    Ok(response)
                };

                match accept_hdr_async(stream, callback).await {
                    Ok(ws_stream) => {
                        metrics.ws_connections_total.inc();
                        metrics.ws_connections_active.inc();

                        let client_id = format!("client_{}", uuid_v4());
                        let (mut write, mut read) = ws_stream.split();

                        // 消息通道
                        let (tx, mut rx) = mpsc::channel::<Message>(100);

                        // 注册客户端
                        {
                            let mut mgr = manager.lock().await;
                            mgr.add_client(&client_id, tx.clone());
                        }

                        info!(client_id = %client_id, addr = %addr, "WebSocket 已连接");

                        // 发送线程：从 rx 接收消息并发送
                        let write_task = tokio::spawn(async move {
                            while let Some(msg) = rx.recv().await {
                                if let Err(e) = write.send(msg).await {
                                    debug!("发送失败: {}", e);
                                    break;
                                }
                            }
                            let _ = write.close().await;
                        });

                        // 读取线程：处理客户端消息
                        let mgr_clone = manager.clone();
                        let cid = client_id.clone();
                        let metrics_clone = metrics.clone();
                        let tx_clone = tx.clone();
                        let read_task = tokio::spawn(async move {
                            let mut hb_count = 0u64;
                            loop {
                                tokio::select! {
                                    msg = read.next() => {
                                        match msg {
                                            Some(Ok(Message::Text(text))) => {
                                                // 检查是否是 ping 消息，用于重置心跳计数
                                                if let Ok(msg) = serde_json::from_str::<ClientMessage>(&text) {
                                                    if msg.action == "ping" {
                                                        hb_count = 0; // 重置心跳计数
                                                        // 回复心跳
                                                        let hb = PushMessage {
                                                            msg_type: PushType::Heartbeat,
                                                            bed_no: 0,
                                                            timestamp: chrono::Utc::now().timestamp_millis(),
                                                            data: None,
                                                        };
                                                        if let Ok(hb_str) = serde_json::to_string(&hb) {
                                                            let _ = tx_clone.send(Message::Text(hb_str)).await;
                                                        }
                                                    } else {
                                                        handle_client_message(&mgr_clone, &cid, &text, &tx_clone).await;
                                                    }
                                                } else {
                                                    handle_client_message(&mgr_clone, &cid, &text, &tx_clone).await;
                                                }
                                            }
                                            Some(Ok(Message::Binary(_))) => {}
                                            Some(Ok(Message::Ping(data))) => {
                                                debug!("收到 Ping");
                                                // Pong 由库自动处理
                                            }
                                            Some(Ok(Message::Pong(_))) => {
                                                hb_count = 0;
                                            }
                                            Some(Ok(Message::Close(_))) => {
                                                info!(client_id = %cid, "客户端主动关闭");
                                                break;
                                            }
                                            Some(Ok(Message::Frame(_))) => {}
                                            Some(Err(e)) => {
                                                warn!(client_id = %cid, error = %e, "WebSocket 错误");
                                                break;
                                            }
                                            None => {
                                                break;
                                            }
                                        }
                                    }
                                    _ = tokio::time::sleep(std::time::Duration::from_secs(heartbeat)) => {
                                        hb_count += 1;
                                        if hb_count > 3 {
                                            warn!(client_id = %cid, "心跳超时，断开连接");
                                            break;
                                        }
                                    }
                                }
                            }

                            // 清理
                            let mut mgr = mgr_clone.lock().await;
                            mgr.remove_client(&cid);
                            metrics_clone.ws_connections_active.dec();
                            info!(client_id = %cid, "WebSocket 已断开");
                        });

                        let _ = tokio::join!(write_task, read_task);
                    }
                    Err(e) => {
                        warn!(client = %addr, error = %e, "WebSocket 握手失败");
                    }
                }
            });
        }

        Ok(())
    }

    fn spawn_vital_broadcaster(&self) {
        let mut rx = self.bus.subscribe_vital();
        let manager = self.manager.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(vital) => {
                        let mut vital_json = serde_json::to_value(&vital).unwrap_or_default();
                        if let Some(obj) = vital_json.as_object_mut() {
                            obj.insert("bed_no".to_string(), serde_json::json!(vital.device.bed_no));
                        }
                        let msg = PushMessage {
                            msg_type: PushType::VitalSigns,
                            bed_no: vital.device.bed_no,
                            timestamp: vital.sample_time.timestamp_millis(),
                            data: Some(vital_json),
                        };
                        let msg_str = serde_json::to_string(&msg).unwrap_or_default();

                        let mgr = manager.lock().await;
                        let sent = mgr.broadcast_to_bed(vital.device.bed_no, &msg_str).await;
                        if sent > 0 {
                            metrics.vital_pushed.inc();
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        warn!("体征广播滞后，丢弃旧消息");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
    }

    fn spawn_waveform_broadcaster(&self) {
        let mut rx = self.bus.subscribe_waveform();
        let manager = self.manager.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(wave) => {
                        let msg = PushMessage {
                            msg_type: PushType::Waveform,
                            bed_no: wave.device.bed_no,
                            timestamp: wave.start_time.timestamp_millis(),
                            data: Some(serde_json::json!([{
                                "device": wave.device,
                                "lead": wave.lead,
                                "sample_rate": wave.sample_rate,
                                "resolution": wave.resolution,
                                "start_time": wave.start_time,
                                "samples": wave.samples,
                                "is_invalid": wave.is_invalid,
                                "bed_no": wave.device.bed_no,
                                "sample_time": wave.start_time,
                            }])),
                        };
                        let msg_str = serde_json::to_string(&msg).unwrap_or_default();

                        let mgr = manager.lock().await;
                        let sent = mgr.broadcast_to_bed(wave.device.bed_no, &msg_str).await;
                        if sent > 0 {
                            metrics.wave_pushed.inc();
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        warn!("波形广播滞后，丢弃旧消息");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
    }

    fn spawn_alarm_broadcaster(&self) {
        let mut rx = self.bus.subscribe_alarm();
        let manager = self.manager.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(alarm) => {
                        let msg = PushMessage {
                            msg_type: PushType::Alarm,
                            bed_no: alarm.device.bed_no,
                            timestamp: alarm.alarm_time.timestamp_millis(),
                            data: Some(serde_json::to_value(&alarm).unwrap_or_default()),
                        };
                        let msg_str = serde_json::to_string(&msg).unwrap_or_default();

                        let mgr = manager.lock().await;
                        let sent = mgr.broadcast_to_bed(alarm.device.bed_no, &msg_str).await;
                        if sent > 0 {
                            metrics.alarm_pushed.inc();
                        }

                        info!(
                            bed_no = alarm.device.bed_no,
                            code = %alarm.alarm_code,
                            phase = %alarm.event_phase,
                            "报警已推送"
                        );
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        warn!("报警广播滞后");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
    }

    pub fn client_manager(&self) -> Arc<Mutex<ClientManager>> {
        self.manager.clone()
    }
}

async fn handle_client_message(
    manager: &Arc<Mutex<ClientManager>>,
    client_id: &str,
    text: &str,
    tx: &mpsc::Sender<Message>,
) {
    if let Ok(msg) = serde_json::from_str::<ClientMessage>(text) {
        match msg.action.as_str() {
            "subscribe" => {
                if let Some(bed_no) = msg.bed_no {
                    let mut mgr = manager.lock().await;
                    mgr.subscribe(client_id, bed_no);
                    info!(client_id, bed_no, "订阅床位");
                    // 发送订阅确认
                    let ack = PushMessage {
                        msg_type: PushType::SubscribeAck,
                        bed_no,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        data: Some(serde_json::json!({ "success": true })),
                    };
                    if let Ok(ack_str) = serde_json::to_string(&ack) {
                        let _ = tx.send(Message::Text(ack_str)).await;
                    }
                }
            }
            "unsubscribe" => {
                if let Some(bed_no) = msg.bed_no {
                    let mut mgr = manager.lock().await;
                    mgr.unsubscribe(client_id, bed_no);
                    info!(client_id, bed_no, "取消订阅床位");
                }
            }
            "ping" => {
                // 回复心跳
                let hb = PushMessage {
                    msg_type: PushType::Heartbeat,
                    bed_no: 0,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    data: None,
                };
                if let Ok(hb_str) = serde_json::to_string(&hb) {
                    let _ = tx.send(Message::Text(hb_str)).await;
                }
            }
            _ => {
                debug!(client_id, action = %msg.action, "未知操作");
            }
        }
    }
}

fn uuid_v4() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let a: u32 = rng.gen();
    let b: u16 = rng.gen();
    let c: u16 = rng.gen::<u16>() | 0x4000; // version 4
    let d: u16 = rng.gen::<u16>() | 0x8000; // variant
    let e: [u8; 6] = rng.gen();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        a, b, c, d, e[0], e[1], e[2], e[3], e[4], e[5]
    )
}

/// 健康检查 Provider
pub struct PushHealthProvider {
    metrics: Arc<PushMetrics>,
    manager: Arc<Mutex<ClientManager>>,
}

impl PushHealthProvider {
    pub fn new(metrics: Arc<PushMetrics>, manager: Arc<Mutex<ClientManager>>) -> Self {
        Self { metrics, manager }
    }
}

impl HealthProvider for PushHealthProvider {
    fn health(&self) -> HealthCheck {
        let client_count = self.manager.try_lock().map(|m| m.client_count()).unwrap_or(0);

        HealthCheck {
            service: "push".to_string(),
            status: HealthStatus::Healthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.metrics.start_time.elapsed().as_secs(),
            components: vec![
                ComponentHealth {
                    name: "connections".to_string(),
                    status: HealthStatus::Healthy,
                    detail: Some(format!("active: {}, total: {}",
                        client_count,
                        self.metrics.ws_connections_total.get()
                    )),
                },
                ComponentHealth {
                    name: "messages".to_string(),
                    status: HealthStatus::Healthy,
                    detail: Some(format!("vital: {}, wave: {}, alarm: {}",
                        self.metrics.vital_pushed.get(),
                        self.metrics.wave_pushed.get(),
                        self.metrics.alarm_pushed.get()
                    )),
                },
            ],
        }
    }
}
