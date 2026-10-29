//! 监护仪监控系统 - 单机一体化版本
//!
//! 单进程包含：
//! - TCP 设备接入（HL7 协议）
//! - 数据存储（SQLite + 本地波形文件）
//! - WebSocket 实时推送
//! - 健康检查接口
//!
//! 零外部依赖，解压即用。

mod bus;
mod config;
mod http_server;
mod storage;
mod tcp_server;
mod ws_server;

use std::sync::Arc;

use anyhow::Result;
use bus::MessageBus;
use clap::Parser;
use config::StandaloneConfig;
use http_server::HttpServer;
use monitor_common::metrics::health_server::HealthServer;
use monitor_common::metrics::{AccessMetrics, PushMetrics};
use storage::SqliteStorage;
use tcp_server::{AccessHealthProvider, TcpAccessService};
use tracing::{error, info};
use ws_server::{PushHealthProvider, WsPushServer};

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "monitor", version, about = "监护仪监控系统 - 单机版")]
struct Args {
    /// 配置文件路径
    #[arg(long, default_value = "config.toml")]
    config: String,

    /// TCP 端口（覆盖配置）
    #[arg(long)]
    tcp_port: Option<u16>,

    /// WebSocket 端口（覆盖配置）
    #[arg(long)]
    ws_port: Option<u16>,

    /// HTTP 静态文件端口（覆盖配置）
    #[arg(long)]
    http_port: Option<u16>,

    /// 数据目录（覆盖配置）
    #[arg(long)]
    data_dir: Option<String>,

    /// 日志级别
    #[arg(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 加载配置
    let config_path = &args.config;
    let mut config = if std::path::Path::new(config_path).exists() {
        StandaloneConfig::from_file(config_path)?
    } else {
        StandaloneConfig::default()
    };

    // 命令行覆盖
    if let Some(port) = args.tcp_port {
        config.tcp_port = port;
    }
    if let Some(port) = args.ws_port {
        config.ws_port = port;
    }
    if let Some(port) = args.http_port {
        config.http_port = port;
    }
    if let Some(dir) = args.data_dir {
        config.data_dir = dir;
    }
    if let Some(level) = args.log_level {
        config.log_level = level;
    }

    // 初始化日志
    init_logging(&config.log_dir, &config.log_level)?;

    // 打印启动信息
    print_banner();
    info!("========================================");
    info!("  监护仪监控系统 - 单机一体化版本");
    info!("========================================");
    info!("TCP 接入端口: {}", config.tcp_port);
    info!("WebSocket 端口: {}", config.ws_port);
    info!("健康检查端口: {}", config.health_port());
    info!("数据目录: {}", config.data_dir);
    info!("日志级别: {}", config.log_level);
    info!("========================================");

    // 确保目录存在
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(&config.wave_dir())?;
    std::fs::create_dir_all(&config.log_dir)?;

    // 指标
    let access_metrics = Arc::new(AccessMetrics::new());
    let push_metrics = Arc::new(PushMetrics::new());

    // 消息总线
    let bus = Arc::new(MessageBus::new(10000));

    // 存储引擎
    let sqlite = Arc::new(SqliteStorage::open(&config.db_path()).await?);

    // 启动存储消费者（仅存储体征数据）
    start_storage_consumer(&bus, sqlite.clone());

    // 启动数据清理任务
    start_cleanup_task(
        sqlite.clone(),
        config.vital_retention_hours,
    );

    // TCP 接入服务
    let tcp_service = TcpAccessService::new(
        config.tcp_listen(),
        bus.clone(),
        access_metrics.clone(),
        config.max_connections,
        config.ip_to_bed.clone(),
    );

    // WebSocket 推送服务
    let ws_service = WsPushServer::new(
        config.ws_listen(),
        bus.clone(),
        push_metrics.clone(),
        config.heartbeat_interval_secs,
    );

    // 健康检查服务
    let access_health = AccessHealthProvider::new(access_metrics.clone());
    let push_health = PushHealthProvider::new(push_metrics.clone(), ws_service.client_manager());

    let combined_health = CombinedHealthProvider::new(
        vec![
            ("access".to_string(), Arc::new(access_health)),
            ("push".to_string(), Arc::new(push_health)),
        ],
        "monitor-standalone",
    );

    let health_server = HealthServer::new(config.health_listen(), Arc::new(combined_health));

    // 启动所有服务
    let health_handle = tokio::spawn(async move {
        if let Err(e) = health_server.run().await {
            error!(error = %e, "健康检查服务异常");
        }
    });

    let tcp_handle = tokio::spawn(async move {
        if let Err(e) = tcp_service.run().await {
            error!(error = %e, "TCP 接入服务异常");
        }
    });

    let ws_handle = tokio::spawn(async move {
        if let Err(e) = ws_service.run().await {
            error!(error = %e, "WebSocket 推送服务异常");
        }
    });

    info!("系统启动完成！");
    info!("设备连接: TCP {}", config.tcp_listen());
    info!("前端连接: ws://{}", config.ws_listen());
    info!("Web 界面: http://localhost:{}", config.http_port);
    info!("健康检查: http://{}/health", config.health_listen());
    info!("指标接口: http://{}/metrics", config.health_listen());

    // 启动HTTP服务（静态文件 + API，如果 web 目录存在）
    let http_handle = if std::path::Path::new(&config.web_dir).exists() {
        let http_server = HttpServer::new(config.http_listen(), &config.web_dir, Some(sqlite.clone()));
        tokio::spawn(async move {
            if let Err(e) = http_server.run().await {
                error!(error = %e, "HTTP 服务异常");
            }
        })
    } else {
        info!("Web 目录不存在，跳过静态文件服务: {}", config.web_dir);
        tokio::spawn(async {})
    };

    // 等待服务运行或收到退出信号（Ctrl+C / 启动器写入的 shutdown.request）
    let shutdown_data_dir = config.data_dir.clone();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("received ctrl-c, shutting down");
        }
        _ = wait_for_shutdown_signal(shutdown_data_dir) => {
            info!("received shutdown request file, shutting down");
        }
        _ = async move {
            let _ = tokio::join!(tcp_handle, ws_handle, health_handle, http_handle);
        } => {}
    }

    Ok(())
}

/// 轮询 shutdown 信号文件：启动器在停止服务前创建 <data_dir>/shutdown.request。
/// 不依赖控制台事件，因此子进程可以无控制台运行。
async fn wait_for_shutdown_signal(data_dir: String) {
    let path = std::path::Path::new(&data_dir).join("shutdown.request");
    loop {
        if path.exists() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// 存储消费者（仅存储体征数据，波形和报警不持久化）
fn start_storage_consumer(
    bus: &Arc<MessageBus>,
    sqlite: Arc<SqliteStorage>,
) {
    let mut vital_rx = bus.subscribe_vital();
    let sqlite_clone = sqlite.clone();

    tokio::spawn(async move {
        loop {
            match vital_rx.recv().await {
                Ok(vital) => {
                    if let Err(e) = sqlite_clone.insert_vital(&vital).await {
                        error!(error = %e, "存储体征数据失败");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("体征存储滞后，丢失 {} 条", n);
                }
                Err(_) => break,
            }
        }
    });
}

/// 数据清理任务（仅清理体征数据）
fn start_cleanup_task(
    sqlite: Arc<SqliteStorage>,
    vital_hours: u32,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;

            info!("开始清理过期数据...");

            match sqlite.cleanup_old_data(vital_hours).await {
                Ok(v) => {
                    if v > 0 {
                        info!("清理完成: 体征 {} 条", v);
                    }
                }
                Err(e) => {
                    error!(error = %e, "清理 SQLite 数据失败");
                }
            }
        }
    });
}

/// 初始化日志
fn init_logging(log_dir: &str, level: &str) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    // 从环境变量设置日志级别
    std::env::set_var("RUST_LOG", level);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .try_init()
        .ok();

    Ok(())
}

fn print_banner() {
    println!(
        r#"
 __  __            _ _
|  \/  | ___  _ __ (_) |_ ___  _ __
| |\/| |/ _ \| '_ \| | __/ _ \| '__|
| |  | | (_) | | | | | || (_) | |
|_|  |_|\___/|_| |_|_|\__\___/|_|
          Monitor System - Standalone
"#
    );
}

use monitor_common::metrics::health_server::HealthProvider;
use monitor_common::metrics::{ComponentHealth, HealthCheck, HealthStatus};

struct CombinedHealthProvider {
    providers: Vec<(String, Arc<dyn HealthProvider + Send + Sync>)>,
    service_name: String,
}

impl CombinedHealthProvider {
    fn new(providers: Vec<(String, Arc<dyn HealthProvider + Send + Sync>)>, service_name: &str) -> Self {
        Self {
            providers,
            service_name: service_name.to_string(),
        }
    }
}

impl HealthProvider for CombinedHealthProvider {
    fn health(&self) -> HealthCheck {
        let mut components = Vec::new();
        let mut overall_status = HealthStatus::Healthy;

        for (name, provider) in &self.providers {
            let h = provider.health();
            match h.status {
                HealthStatus::Unhealthy => {
                    overall_status = HealthStatus::Unhealthy;
                }
                HealthStatus::Degraded => {
                    if matches!(overall_status, HealthStatus::Healthy) {
                        overall_status = HealthStatus::Degraded;
                    }
                }
                _ => {}
            }
            components.push(ComponentHealth {
                name: name.clone(),
                status: h.status,
                detail: None,
            });
        }

        HealthCheck {
            service: self.service_name.clone(),
            status: overall_status,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: 0,
            components,
        }
    }
}

// 为了让 AccessMetrics 和 PushMetrics 编译通过，需要有 start_time 字段
// 让我确认一下 AccessMetrics 的定义
