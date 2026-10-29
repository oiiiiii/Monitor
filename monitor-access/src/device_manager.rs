//! 设备管理器
//!
//! 负责管理所有设备连接，提供：
//! - 设备注册/注销
//! - 连接数统计
//! - 设备信息查询

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::event::DisconnectReason;

/// 设备连接信息
#[derive(Debug, Clone)]
pub struct DeviceConnectionInfo {
    /// 设备 IP
    pub device_ip: String,
    /// 连接建立时间
    pub connect_time: Instant,
    /// 最后活动时间
    pub last_active_time: Instant,
    /// 接收的消息数
    pub message_count: u64,
}

/// 设备管理器
#[derive(Clone)]
pub struct DeviceManager {
    inner: Arc<Inner>,
}

struct Inner {
    /// 设备连接表（IP -> 连接信息）
    connections: RwLock<HashMap<String, DeviceConnectionInfo>>,
    /// 最大连接数
    max_connections: usize,
}

impl DeviceManager {
    /// 创建新的设备管理器
    pub fn new(max_connections: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                connections: RwLock::new(HashMap::new()),
                max_connections,
            }),
        }
    }

    /// 注册设备连接
    ///
    /// 返回 Ok(()) 表示注册成功，Err(()) 表示连接数已满
    pub async fn register_device(&self, device_ip: &str) -> Result<(), ()> {
        let mut connections = self.inner.connections.write().await;

        // 检查是否已存在（同一设备重连）
        if connections.contains_key(device_ip) {
            warn!(ip = %device_ip, "设备已存在，将替换旧连接");
        }

        // 检查连接数限制（如果是新设备）
        if !connections.contains_key(device_ip) && connections.len() >= self.inner.max_connections {
            warn!(
                ip = %device_ip,
                current = connections.len(),
                max = self.inner.max_connections,
                "连接数已达上限，拒绝新连接"
            );
            return Err(());
        }

        let now = Instant::now();
        let info = DeviceConnectionInfo {
            device_ip: device_ip.to_string(),
            connect_time: now,
            last_active_time: now,
            message_count: 0,
        };

        connections.insert(device_ip.to_string(), info);

        info!(
            ip = %device_ip,
            total = connections.len(),
            "设备上线"
        );

        Ok(())
    }

    /// 注销设备连接
    pub async fn unregister_device(&self, device_ip: &str, reason: DisconnectReason) {
        let mut connections = self.inner.connections.write().await;

        if let Some(info) = connections.remove(device_ip) {
            let duration = info.connect_time.elapsed();
            info!(
                ip = %device_ip,
                reason = %reason,
                duration_secs = duration.as_secs(),
                messages = info.message_count,
                total = connections.len(),
                "设备下线"
            );
        } else {
            debug!(ip = %device_ip, "注销未知设备");
        }
    }

    /// 更新设备最后活动时间
    pub async fn update_activity(&self, device_ip: &str) {
        let mut connections = self.inner.connections.write().await;

        if let Some(info) = connections.get_mut(device_ip) {
            info.last_active_time = Instant::now();
        }
    }

    /// 增加消息计数
    pub async fn increment_message_count(&self, device_ip: &str) {
        let mut connections = self.inner.connections.write().await;

        if let Some(info) = connections.get_mut(device_ip) {
            info.message_count += 1;
        }
    }

    /// 获取当前连接数
    pub async fn connection_count(&self) -> usize {
        let connections = self.inner.connections.read().await;
        connections.len()
    }

    /// 获取所有设备 IP 列表
    pub async fn device_ips(&self) -> Vec<String> {
        let connections = self.inner.connections.read().await;
        connections.keys().cloned().collect()
    }

    /// 获取指定设备的连接信息
    pub async fn get_connection_info(&self, device_ip: &str) -> Option<DeviceConnectionInfo> {
        let connections = self.inner.connections.read().await;
        connections.get(device_ip).cloned()
    }

    /// 获取最大连接数
    pub fn max_connections(&self) -> usize {
        self.inner.max_connections
    }
}
