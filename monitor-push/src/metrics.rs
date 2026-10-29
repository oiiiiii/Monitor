//! 推送层指标管理
//!
//! 集成 metrics 模块，提供推送层的健康检查和指标快照。

use std::sync::Arc;

use monitor_common::metrics::{
    PushMetrics, PushMetricsSnapshot, ComponentHealth, HealthCheck, HealthStatus,
    health_server::HealthProvider,
};

/// 推送层健康提供器
pub struct PushHealthProvider {
    pub metrics: Arc<PushMetrics>,
    pub redis_healthy: std::sync::atomic::AtomicBool,
    pub mq_healthy: std::sync::atomic::AtomicBool,
}

impl PushHealthProvider {
    pub fn new(metrics: Arc<PushMetrics>) -> Self {
        Self {
            metrics,
            redis_healthy: std::sync::atomic::AtomicBool::new(true),
            mq_healthy: std::sync::atomic::AtomicBool::new(true),
        }
    }

    pub fn set_redis_healthy(&self, healthy: bool) {
        self.redis_healthy.store(healthy, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_mq_healthy(&self, healthy: bool) {
        self.mq_healthy.store(healthy, std::sync::atomic::Ordering::Relaxed);
    }
}

impl HealthProvider for PushHealthProvider {
    fn health(&self) -> HealthCheck {
        let metrics = self.metrics.snapshot();
        let mut components = Vec::new();

        // WebSocket 服务
        components.push(ComponentHealth {
            name: "websocket".to_string(),
            status: HealthStatus::Healthy,
            detail: Some(format!(
                "活跃连接: {}, 累计连接: {}",
                metrics.ws_connections_active, metrics.ws_connections_total
            )),
        });

        // Redis
        let redis_ok = self.redis_healthy.load(std::sync::atomic::Ordering::Relaxed);
        components.push(ComponentHealth {
            name: "redis".to_string(),
            status: if redis_ok {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded
            },
            detail: Some(format!(
                "命中: {}, 未命中: {}, 命中率: {:.2}%",
                metrics.cache_hits,
                metrics.cache_misses,
                metrics.cache_hit_rate * 100.0
            )),
        });

        // MQ 消费者
        let mq_ok = self.mq_healthy.load(std::sync::atomic::Ordering::Relaxed);
        components.push(ComponentHealth {
            name: "mq_consumer".to_string(),
            status: if mq_ok {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy
            },
            detail: Some(format!(
                "体征推送: {}, 波形推送: {}, 报警推送: {}",
                metrics.vital_pushed, metrics.wave_pushed, metrics.alarm_pushed
            )),
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
            service: "monitor-push".to_string(),
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
