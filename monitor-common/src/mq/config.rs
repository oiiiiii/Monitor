//! RabbitMQ 配置

use serde::{Deserialize, Serialize};
use config as config_crate;

/// RabbitMQ 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqConfig {
    /// 主机地址
    #[serde(default = "default_host")]
    pub host: String,

    /// 端口
    #[serde(default = "default_port")]
    pub port: u16,

    /// 用户名
    #[serde(default = "default_username")]
    pub username: String,

    /// 密码
    #[serde(default = "default_password")]
    pub password: String,

    /// 虚拟主机
    #[serde(default = "default_vhost")]
    pub vhost: String,

    /// 交换机名称
    #[serde(default = "default_exchange")]
    pub exchange: String,

    /// 连接超时时间（秒）
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,

    /// 生产者确认超时时间（毫秒）
    #[serde(default = "default_confirm_timeout")]
    pub confirm_timeout_ms: u64,

    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// 重试间隔（毫秒）
    #[serde(default = "default_retry_interval")]
    pub retry_interval_ms: u64,

    /// 预取数量（消费者用）
    #[serde(default = "default_prefetch_count")]
    pub prefetch_count: u16,
}

impl Default for MqConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            username: default_username(),
            password: default_password(),
            vhost: default_vhost(),
            exchange: default_exchange(),
            connection_timeout_secs: default_connection_timeout(),
            confirm_timeout_ms: default_confirm_timeout(),
            max_retries: default_max_retries(),
            retry_interval_ms: default_retry_interval(),
            prefetch_count: default_prefetch_count(),
        }
    }
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    5672
}

fn default_username() -> String {
    "guest".to_string()
}

fn default_password() -> String {
    "guest".to_string()
}

fn default_vhost() -> String {
    "/".to_string()
}

fn default_exchange() -> String {
    "ecg_exchange".to_string()
}

fn default_connection_timeout() -> u64 {
    30
}

fn default_confirm_timeout() -> u64 {
    5000
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_interval() -> u64 {
    1000
}

fn default_prefetch_count() -> u16 {
    100
}

impl MqConfig {
    /// 获取 AMQP 连接 URL
    pub fn amqp_url(&self) -> String {
        format!(
            "amqp://{}:{}@{}:{}{}",
            self.username,
            self.password,
            self.host,
            self.port,
            self.vhost
        )
    }

    /// 从环境变量加载配置
    pub fn from_env() -> anyhow::Result<Self> {
        let config = config_crate::Config::builder()
            .add_source(config_crate::Environment::with_prefix("MONITOR_MQ"))
            .build()?;

        let mut cfg: MqConfig = config.try_deserialize().unwrap_or_default();

        // 合并默认值
        if cfg.host.is_empty() {
            cfg.host = default_host();
        }
        if cfg.port == 0 {
            cfg.port = default_port();
        }
        if cfg.username.is_empty() {
            cfg.username = default_username();
        }
        if cfg.password.is_empty() {
            cfg.password = default_password();
        }
        if cfg.vhost.is_empty() {
            cfg.vhost = default_vhost();
        }
        if cfg.exchange.is_empty() {
            cfg.exchange = default_exchange();
        }
        if cfg.connection_timeout_secs == 0 {
            cfg.connection_timeout_secs = default_connection_timeout();
        }
        if cfg.confirm_timeout_ms == 0 {
            cfg.confirm_timeout_ms = default_confirm_timeout();
        }
        if cfg.max_retries == 0 {
            cfg.max_retries = default_max_retries();
        }
        if cfg.retry_interval_ms == 0 {
            cfg.retry_interval_ms = default_retry_interval();
        }
        if cfg.prefetch_count == 0 {
            cfg.prefetch_count = default_prefetch_count();
        }

        Ok(cfg)
    }
}
