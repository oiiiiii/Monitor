//! 单机版配置

use serde::{Deserialize, Serialize};

/// 单机版配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandaloneConfig {
    /// TCP 监听地址（设备接入）
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// TCP 监听端口
    #[serde(default = "default_tcp_port")]
    pub tcp_port: u16,

    /// WebSocket 监听端口
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,

    /// 健康检查端口（基于 TCP 端口 + 100）
    #[serde(default)]
    pub health_port: Option<u16>,

    /// HTTP 静态文件端口（默认 3000）
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    /// 最大连接数
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// 数据目录
    #[serde(default = "default_data_dir")]
    pub data_dir: String,

    /// 日志目录
    #[serde(default = "default_log_dir")]
    pub log_dir: String,

    /// Web 前端目录
    #[serde(default = "default_web_dir")]
    pub web_dir: String,

    /// 日志级别
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// 波形保留分钟数
    #[serde(default = "default_wave_retention_minutes")]
    pub wave_retention_minutes: u32,

    /// 体征保留小时数
    #[serde(default = "default_vital_retention_hours")]
    pub vital_retention_hours: u32,

    /// 报警保留分钟数
    #[serde(default = "default_alarm_retention_minutes")]
    pub alarm_retention_minutes: u32,

    /// WebSocket 心跳间隔（秒）
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,

    /// IP 到床位号的映射（手动配置）
    #[serde(default)]
    pub ip_to_bed: std::collections::HashMap<String, u16>,
}

fn default_listen_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_tcp_port() -> u16 {
    9000
}

fn default_ws_port() -> u16 {
    8080
}

fn default_http_port() -> u16 {
    3000
}

fn default_max_connections() -> usize {
    100
}

fn default_data_dir() -> String {
    "./data".to_string()
}

fn default_log_dir() -> String {
    "./logs".to_string()
}

fn default_web_dir() -> String {
    "./web".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_wave_retention_minutes() -> u32 {
    5
}

fn default_vital_retention_hours() -> u32 {
    24
}

fn default_alarm_retention_minutes() -> u32 {
    5
}

fn default_heartbeat_interval() -> u64 {
    30
}

impl Default for StandaloneConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            tcp_port: default_tcp_port(),
            ws_port: default_ws_port(),
            health_port: None,
            http_port: default_http_port(),
            max_connections: default_max_connections(),
            data_dir: default_data_dir(),
            log_dir: default_log_dir(),
            web_dir: default_web_dir(),
            log_level: default_log_level(),
            wave_retention_minutes: default_wave_retention_minutes(),
            vital_retention_hours: default_vital_retention_hours(),
            alarm_retention_minutes: default_alarm_retention_minutes(),
            heartbeat_interval_secs: default_heartbeat_interval(),
            ip_to_bed: std::collections::HashMap::new(),
        }
    }
}

impl StandaloneConfig {
    /// 健康检查端口
    pub fn health_port(&self) -> u16 {
        self.health_port.unwrap_or(self.tcp_port + 100)
    }

    /// TCP 监听地址
    pub fn tcp_listen(&self) -> String {
        format!("{}:{}", self.listen_addr, self.tcp_port)
    }

    /// WebSocket 监听地址
    pub fn ws_listen(&self) -> String {
        format!("{}:{}", self.listen_addr, self.ws_port)
    }

    /// 健康检查监听地址
    pub fn health_listen(&self) -> String {
        format!("{}:{}", self.listen_addr, self.health_port())
    }

    /// HTTP 静态文件监听地址
    pub fn http_listen(&self) -> String {
        format!("{}:{}", self.listen_addr, self.http_port)
    }

    /// SQLite 数据库路径
    pub fn db_path(&self) -> String {
        format!("{}/monitor.db", self.data_dir)
    }

    /// 波形数据目录
    pub fn wave_dir(&self) -> String {
        format!("{}/wave", self.data_dir)
    }

    /// 波形保留小时数（供兼容性使用）
    pub fn wave_retention_hours(&self) -> u32 {
        (self.wave_retention_minutes + 59) / 60
    }

    /// 报警保留小时数（供兼容性使用）
    pub fn alarm_retention_hours(&self) -> u32 {
        (self.alarm_retention_minutes + 59) / 60
    }

    /// 从 TOML 文件加载配置
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: StandaloneConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// 从环境变量加载（覆盖文件配置）
    pub fn with_env_overrides(mut self) -> anyhow::Result<Self> {
        if let Ok(v) = std::env::var("MONITOR_TCP_PORT") {
            if let Ok(p) = v.parse::<u16>() {
                self.tcp_port = p;
            }
        }
        if let Ok(v) = std::env::var("MONITOR_WS_PORT") {
            if let Ok(p) = v.parse::<u16>() {
                self.ws_port = p;
            }
        }
        if let Ok(v) = std::env::var("MONITOR_DATA_DIR") {
            self.data_dir = v;
        }
        if let Ok(v) = std::env::var("MONITOR_LOG_LEVEL") {
            self.log_level = v;
        }
        Ok(self)
    }
}
