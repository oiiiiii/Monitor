//! 系统错误类型定义

use thiserror::Error;

/// 系统通用错误类型
#[derive(Error, Debug)]
pub enum MonitorError {
    /// 协议解析错误
    #[error("协议解析错误: {0}")]
    ParseError(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),

    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigError(String),

    /// 数据库错误
    #[error("数据库错误: {0}")]
    DatabaseError(String),

    /// 网络错误
    #[error("网络错误: {0}")]
    NetworkError(String),

    /// 其他错误
    #[error("其他错误: {0}")]
    Other(String),
}

pub type MonitorResult<T> = Result<T, MonitorError>;
