//! 消费服务配置

use serde::{Deserialize, Serialize};

/// 消费服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// 消费者组名称前缀
    #[serde(default = "default_consumer_group_prefix")]
    pub consumer_group_prefix: String,

    /// 预取数量（QoS）
    #[serde(default = "default_prefetch_count")]
    pub prefetch_count: u16,

    /// 最大重试次数（消费失败重试）
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// 重试间隔（毫秒）
    #[serde(default = "default_retry_interval_ms")]
    pub retry_interval_ms: u64,

    /// 波形批量写入大小
    #[serde(default = "default_wave_batch_size")]
    pub wave_batch_size: usize,

    /// 波形批量写入最大等待时间（毫秒）
    #[serde(default = "default_wave_batch_max_wait_ms")]
    pub wave_batch_max_wait_ms: u64,

    /// TDengine 配置
    #[serde(default)]
    pub tdengine: TdengineConfig,

    /// PostgreSQL 配置
    #[serde(default)]
    pub postgres: PostgresConfig,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_group_prefix: default_consumer_group_prefix(),
            prefetch_count: default_prefetch_count(),
            max_retries: default_max_retries(),
            retry_interval_ms: default_retry_interval_ms(),
            wave_batch_size: default_wave_batch_size(),
            wave_batch_max_wait_ms: default_wave_batch_max_wait_ms(),
            tdengine: TdengineConfig::default(),
            postgres: PostgresConfig::default(),
        }
    }
}

fn default_consumer_group_prefix() -> String {
    "monitor_consumer".to_string()
}

fn default_prefetch_count() -> u16 {
    100
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_interval_ms() -> u64 {
    1000
}

fn default_wave_batch_size() -> usize {
    100
}

fn default_wave_batch_max_wait_ms() -> u64 {
    1000
}

/// TDengine 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdengineConfig {
    /// 主机地址
    #[serde(default = "default_td_host")]
    pub host: String,

    /// HTTP 端口
    #[serde(default = "default_td_port")]
    pub port: u16,

    /// 用户名
    #[serde(default = "default_td_user")]
    pub username: String,

    /// 密码
    #[serde(default = "default_td_password")]
    pub password: String,

    /// 数据库名
    #[serde(default = "default_td_database")]
    pub database: String,

    /// 连接超时（秒）
    #[serde(default = "default_td_timeout")]
    pub timeout_secs: u64,
}

impl Default for TdengineConfig {
    fn default() -> Self {
        Self {
            host: default_td_host(),
            port: default_td_port(),
            username: default_td_user(),
            password: default_td_password(),
            database: default_td_database(),
            timeout_secs: default_td_timeout(),
        }
    }
}

fn default_td_host() -> String {
    "localhost".to_string()
}

fn default_td_port() -> u16 {
    6041
}

fn default_td_user() -> String {
    "root".to_string()
}

fn default_td_password() -> String {
    "taosdata".to_string()
}

fn default_td_database() -> String {
    "ecg_monitor".to_string()
}

fn default_td_timeout() -> u64 {
    30
}

impl TdengineConfig {
    /// REST API 基础 URL
    pub fn rest_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// PostgreSQL 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// 主机地址
    #[serde(default = "default_pg_host")]
    pub host: String,

    /// 端口
    #[serde(default = "default_pg_port")]
    pub port: u16,

    /// 用户名
    #[serde(default = "default_pg_user")]
    pub username: String,

    /// 密码
    #[serde(default = "default_pg_password")]
    pub password: String,

    /// 数据库名
    #[serde(default = "default_pg_database")]
    pub database: String,

    /// 连接池大小
    #[serde(default = "default_pg_pool_size")]
    pub pool_size: usize,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: default_pg_host(),
            port: default_pg_port(),
            username: default_pg_user(),
            password: default_pg_password(),
            database: default_pg_database(),
            pool_size: default_pg_pool_size(),
        }
    }
}

fn default_pg_host() -> String {
    "localhost".to_string()
}

fn default_pg_port() -> u16 {
    5432
}

fn default_pg_user() -> String {
    "postgres".to_string()
}

fn default_pg_password() -> String {
    "postgres".to_string()
}

fn default_pg_database() -> String {
    "ecg_monitor".to_string()
}

fn default_pg_pool_size() -> usize {
    10
}

impl PostgresConfig {
    /// 数据库连接 URL
    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database
        )
    }
}

impl ConsumerConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> anyhow::Result<Self> {
        let cfg = config::Config::builder()
            .add_source(config::Environment::with_prefix("MONITOR_CONSUMER"))
            .build()?;

        let mut config: ConsumerConfig = cfg.try_deserialize().unwrap_or_default();

        // 合并默认值
        if config.consumer_group_prefix.is_empty() {
            config.consumer_group_prefix = default_consumer_group_prefix();
        }
        if config.prefetch_count == 0 {
            config.prefetch_count = default_prefetch_count();
        }
        if config.max_retries == 0 {
            config.max_retries = default_max_retries();
        }
        if config.retry_interval_ms == 0 {
            config.retry_interval_ms = default_retry_interval_ms();
        }
        if config.wave_batch_size == 0 {
            config.wave_batch_size = default_wave_batch_size();
        }
        if config.wave_batch_max_wait_ms == 0 {
            config.wave_batch_max_wait_ms = default_wave_batch_max_wait_ms();
        }

        Ok(config)
    }
}
