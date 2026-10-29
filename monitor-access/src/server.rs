//! TCP 服务器
//!
//! 监听设备连接，为每个连接创建独立的处理任务，
//! 并将事件发送到事件通道。

use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::AccessConfig;
use crate::connection::DeviceConnection;
use crate::device_manager::DeviceManager;
use crate::event::{AccessEvent, DisconnectReason};

/// TCP 服务器
pub struct AccessServer {
    config: AccessConfig,
    device_manager: DeviceManager,
    event_tx: mpsc::Sender<AccessEvent>,
}

impl AccessServer {
    /// 创建新的 TCP 服务器
    pub fn new(config: AccessConfig, event_tx: mpsc::Sender<AccessEvent>) -> Self {
        let device_manager = DeviceManager::new(config.max_connections);
        Self {
            config,
            device_manager,
            event_tx,
        }
    }

    /// 获取设备管理器引用
    pub fn device_manager(&self) -> &DeviceManager {
        &self.device_manager
    }

    /// 启动服务器（阻塞直到服务器停止）
    pub async fn run(&self) -> anyhow::Result<()> {
        let listen_addr = self.config.listen_string();
        let listener = TcpListener::bind(&listen_addr).await?;

        info!(addr = %listen_addr, "设备接入服务启动，监听中...");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let device_ip = addr.ip().to_string();
                    info!(ip = %device_ip, "新设备连接");

                    // 检查连接数限制
                    if self.device_manager.register_device(&device_ip).await.is_err() {
                        warn!(ip = %device_ip, "连接数已满，拒绝连接");
                        // 主动关闭连接
                        drop(stream);
                        continue;
                    }

                    // 发送设备上线事件
                    self.send_event(AccessEvent::DeviceConnected {
                        device_ip: device_ip.clone(),
                        connect_time: std::time::Instant::now(),
                    }).await;

                    // 为每个连接创建独立任务
                    let dm = self.device_manager.clone();
                    let event_tx = self.event_tx.clone();
                    let event_tx2 = event_tx.clone();
                    let timeout = Duration::from_secs(self.config.connection_timeout_secs);
                    let recv_buffer_size = self.config.recv_buffer_size;

                    tokio::spawn(async move {
                        let reason = Self::handle_connection(
                            stream,
                            device_ip.clone(),
                            event_tx,
                            timeout,
                            recv_buffer_size,
                        ).await;

                        // 发送设备下线事件
                        let _ = event_tx2.send(AccessEvent::DeviceDisconnected {
                            device_ip: device_ip.clone(),
                            reason: reason.clone(),
                        }).await;

                        // 注销设备
                        dm.unregister_device(&device_ip, reason).await;
                    });
                }
                Err(e) => {
                    error!(error = %e, "接受连接失败");
                    // 短暂等待后继续
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// 处理单个连接
    async fn handle_connection(
        stream: TcpStream,
        device_ip: String,
        event_tx: mpsc::Sender<AccessEvent>,
        timeout: Duration,
        recv_buffer_size: usize,
    ) -> DisconnectReason {
        let (reader, _writer) = stream.into_split();

        let mut conn = DeviceConnection::new(
            device_ip,
            event_tx,
            timeout,
            recv_buffer_size,
        );

        conn.run_read_loop(reader).await
    }

    /// 发送事件
    async fn send_event(&self, event: AccessEvent) {
        if self.event_tx.send(event).await.is_err() {
            error!("事件通道已关闭");
        }
    }
}
