//! 接入层指标管理
//!
//! 集成 metrics 模块，提供接入层的健康检查和指标快照。

use std::sync::Arc;

use monitor_common::metrics::{
    AccessMetrics, AccessMetricsSnapshot, ComponentHealth, HealthCheck, HealthStatus,
    health_server::HealthProvider,
};

/// 接入层健康提供器
pub struct AccessHealthProvider {
    /// 指标数据
    pub metrics: Arc<AccessMetrics>,
    /// MQ 是否可用
    pub mq_healthy: std::sync::atomic::AtomicBool,
    /// 磁盘缓冲是否启用
    pub disk_buffer_enabled: bool,
}

impl AccessHealthProvider {
    pub fn new(metrics: Arc<AccessMetrics>) -> Self {
        Self {
            metrics,
            mq_healthy: std::sync::atomic::AtomicBool::new(true),
            disk_buffer_enabled: true,
        }
    }

    /// 设置 MQ 健康状态
    pub fn set_mq_healthy(&self, healthy: bool) {
        self.mq_healthy.store(healthy, std::sync::atomic::Ordering::Relaxed);
    }
}

impl HealthProvider for AccessHealthProvider {
    fn health(&self) -> HealthCheck {
        let metrics = self.metrics.snapshot();
        let mut components = Vec::new();

        // TCP 服务器状态（有活跃连接就是正常的）
        components.push(ComponentHealth {
            name: "tcp_server".to_string(),
            status: HealthStatus::Healthy,
            detail: Some(format!(
                "活跃连接: {}, 累计连接: {}",
                metrics.connections_active, metrics.connections_total
            )),
        });

        // MQ 状态
        let mq_ok = self.mq_healthy.load(std::sync::atomic::Ordering::Relaxed);
        components.push(ComponentHealth {
            name: "rabbitmq".to_string(),
            status: if mq_ok {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded
            },
            detail: Some(format!(
                "已发送: {}, 错误: {}",
                metrics.mq_sent, metrics.mq_send_errors
            )),
        });

        // 协议解析状态
        let parse_error_rate = if metrics.frames_received > 0 {
            metrics.parse_errors as f64 / metrics.frames_received as f64
        } else {
            0.0
        };
        components.push(ComponentHealth {
            name: "protocol_parser".to_string(),
            status: if parse_error_rate < 0.01 {
                HealthStatus::Healthy
            } else if parse_error_rate < 0.1 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unhealthy
            },
            detail: Some(format!(
                "接收帧: {}, 解析成功: {}, 错误: {} ({:.2}%)",
                metrics.frames_received,
                metrics.messages_parsed,
                metrics.parse_errors,
                parse_error_rate * 100.0
            )),
        });

        // 磁盘缓冲状态
        components.push(ComponentHealth {
            name: "disk_buffer".to_string(),
            status: if self.disk_buffer_enabled {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded
            },
            detail: Some(format!("写入次数: {}", metrics.disk_buffer_writes)),
        });

        // 计算整体状态
        let overall = if components.iter().all(|c| c.status == HealthStatus::Healthy) {
            HealthStatus::Healthy
        } else if components.iter().any(|c| c.status == HealthStatus::Unhealthy) {
            HealthStatus::Unhealthy
        } else {
            HealthStatus::Degraded
        };

        HealthCheck {
            status: overall,
            service: "monitor-access".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: metrics.uptime_secs,
            components,
        }
    }

    fn metrics_json(&self) -> String {
        let snapshot = self.metrics.snapshot();
        serde_json::to_string_pretty(&snapshot).unwrap_or_default()
    }
}
