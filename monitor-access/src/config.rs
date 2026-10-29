//! 设备接入服务配置

use serde::{Deserialize, Serialize};

/// 设备接入服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessConfig {
    /// TCP 监听地址
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// TCP 监听端口
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,

    /// 连接超时时间（秒）
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,

    /// 最大连接数
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// 接收缓冲区大小（字节）
    #[serde(default = "default_recv_buffer_size")]
    pub recv_buffer_size: usize,
}

impl Default for AccessConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            listen_port: default_listen_port(),
            connection_timeout_secs: default_connection_timeout(),
            max_connections: default_max_connections(),
            recv_buffer_size: default_recv_buffer_size(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_listen_port() -> u16 {
    9000
}

fn default_connection_timeout() -> u64 {
    300
}

fn default_max_connections() -> usize {
    1000
}

fn default_recv_buffer_size() -> usize {
    8192
}

impl AccessConfig {
    /// 获取监听地址字符串
    pub fn listen_string(&self) -> String {
        format!("{}:{}", self.listen_addr, self.listen_port)
    }

    /// 从环境变量或默认值加载配置
    pub fn load() -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::Environment::with_prefix("MONITOR_ACCESS"))
            .build()?;

        let mut cfg: AccessConfig = config.try_deserialize().unwrap_or_default();

        // 合并默认值
        if cfg.listen_addr.is_empty() {
            cfg.listen_addr = default_listen_addr();
        }
        if cfg.listen_port == 0 {
            cfg.listen_port = default_listen_port();
        }
        if cfg.connection_timeout_secs == 0 {
            cfg.connection_timeout_secs = default_connection_timeout();
        }
        if cfg.max_connections == 0 {
            cfg.max_connections = default_max_connections();
        }
        if cfg.recv_buffer_size == 0 {
            cfg.recv_buffer_size = default_recv_buffer_size();
        }

        Ok(cfg)
    }
}
