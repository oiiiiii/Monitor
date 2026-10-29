//! 推送服务配置

use serde::{Deserialize, Serialize};

/// 推送服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// WebSocket 监听地址
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// WebSocket 监听端口
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,

    /// 心跳间隔（秒）
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,

    /// 最大连接数
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// 波形分片大小（毫秒）
    #[serde(default = "default_wave_slice_ms")]
    pub wave_slice_ms: u32,

    /// Redis 配置
    #[serde(default)]
    pub redis: RedisConfig,
}

/// Redis 配置（简化版，使用 monitor-common 的配置结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// 主机地址
    #[serde(default = "default_redis_host")]
    pub host: String,
    /// 端口
    #[serde(default = "default_redis_port")]
    pub port: u16,
    /// 密码
    #[serde(default)]
    pub password: Option<String>,
    /// 数据库编号
    #[serde(default = "default_redis_db")]
    pub db: i64,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            listen_port: default_listen_port(),
            heartbeat_interval_secs: default_heartbeat_interval(),
            max_connections: default_max_connections(),
            wave_slice_ms: default_wave_slice_ms(),
            redis: RedisConfig::default(),
        }
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            host: default_redis_host(),
            port: default_redis_port(),
            password: None,
            db: default_redis_db(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_listen_port() -> u16 {
    8080
}

fn default_heartbeat_interval() -> u64 {
    30
}

fn default_max_connections() -> usize {
    1000
}

fn default_wave_slice_ms() -> u32 {
    200
}

fn default_redis_host() -> String {
    "localhost".to_string()
}

fn default_redis_port() -> u16 {
    6379
}

fn default_redis_db() -> i64 {
    0
}

impl PushConfig {
    /// 获取监听地址字符串
    pub fn listen_string(&self) -> String {
        format!("{}:{}", self.listen_addr, self.listen_port)
    }

    /// 转换为 monitor-common 的 RedisConfig
    pub fn to_common_redis_config(&self) -> monitor_common::cache::RedisConfig {
        monitor_common::cache::RedisConfig {
            host: self.redis.host.clone(),
            port: self.redis.port,
            password: self.redis.password.clone(),
            db: self.redis.db,
            timeout_secs: 5,
            wave_cache_seconds: 10,
        }
    }

    /// 从环境变量加载配置
    pub fn from_env() -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::Environment::with_prefix("MONITOR_PUSH"))
            .build()?;

        let mut cfg: PushConfig = config.try_deserialize().unwrap_or_default();

        // 合并默认值
        if cfg.listen_addr.is_empty() {
            cfg.listen_addr = default_listen_addr();
        }
        if cfg.listen_port == 0 {
            cfg.listen_port = default_listen_port();
        }
        if cfg.heartbeat_interval_secs == 0 {
            cfg.heartbeat_interval_secs = default_heartbeat_interval();
        }
        if cfg.max_connections == 0 {
            cfg.max_connections = default_max_connections();
        }
        if cfg.wave_slice_ms == 0 {
            cfg.wave_slice_ms = default_wave_slice_ms();
        }

        Ok(cfg)
    }
}
