//! 健康检查 HTTP 服务器
//!
//! 提供轻量级的 HTTP 健康检查端点：
//! - GET /health     → 健康状态（JSON）
//! - GET /metrics    → 指标快照（JSON）
//!
//! 使用 Tokio TcpListener 手写 HTTP/1.1，零额外依赖。

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use super::{HealthCheck, HealthStatus};

/// 健康检查提供器
pub trait HealthProvider: Send + Sync + 'static {
    /// 获取健康状态
    fn health(&self) -> HealthCheck;

    /// 获取指标 JSON（可选，默认返回健康检查的 JSON）
    fn metrics_json(&self) -> String {
        serde_json::to_string_pretty(&self.health()).unwrap_or_default()
    }
}

/// HTTP 健康检查服务器
pub struct HealthServer {
    listen_addr: String,
    provider: Arc<dyn HealthProvider>,
}

impl HealthServer {
    /// 创建新的健康检查服务器
    pub fn new(listen_addr: String, provider: Arc<dyn HealthProvider>) -> Self {
        Self {
            listen_addr,
            provider,
        }
    }

    /// 启动服务器（异步运行，永不返回除非出错）
    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!(addr = %self.listen_addr, "健康检查 HTTP 服务启动");

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let provider = self.provider.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, provider).await {
                            warn!(error = %e, "处理健康检查请求失败");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "接受健康检查连接失败");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

/// 处理单个 HTTP 连接
async fn handle_connection(
    mut stream: TcpStream,
    provider: Arc<dyn HealthProvider>,
) -> anyhow::Result<()> {
    // 读取请求（限制 4KB）
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).await?;

    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");

    // 简单路由
    let (status, content_type, body) = if first_line.starts_with("GET /health") {
        let health = provider.health();
        let status_code = match health.status {
            HealthStatus::Healthy => "200 OK",
            HealthStatus::Degraded => "200 OK",
            HealthStatus::Unhealthy => "503 Service Unavailable",
        };
        (
            status_code,
            "application/json",
            serde_json::to_string_pretty(&health).unwrap_or_default(),
        )
    } else if first_line.starts_with("GET /metrics") {
        ("200 OK", "application/json", provider.metrics_json())
    } else {
        (
            "404 Not Found",
            "text/plain",
            "Not Found".to_string(),
        )
    };

    // 构造 HTTP 响应
    let response = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n\
         {}",
        status,
        content_type,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}
