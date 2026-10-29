//! WebSocket 推送服务入口
//!
//! 从 MQ 消费实时数据，更新 Redis 缓存，
//! 并通过 WebSocket 推送给订阅的前端客户端。
//!
//! 推送优先级：
//! 1. 报警事件（立即推送）
//! 2. 生命体征（准实时）
//! 3. 波形数据（分片推送）

mod config;
mod metrics;
mod pipeline;
mod websocket;

use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use monitor_common::cache::RedisCache;
use monitor_common::mq::MqConfig;
use monitor_common::metrics::{PushMetrics, health_server::HealthServer};

use config::PushConfig;
use metrics::PushHealthProvider;
use pipeline::PushPipeline;
use websocket::WebSocketServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    init_logging();

    info!("========================================");
    info!("  多床位心电监护系统 - 实时推送服务");
    info!("========================================");

    // 加载配置
    let push_config = PushConfig::from_env()?;
    let mq_config = MqConfig::from_env()?;
    let redis_config = push_config.to_common_redis_config();

    info!(
        "WebSocket 监听: {}:{}",
        push_config.listen_addr, push_config.listen_port
    );
    info!("心跳间隔: {}s", push_config.heartbeat_interval_secs);
    info!("最大连接数: {}", push_config.max_connections);
    info!("波形分片: {}ms", push_config.wave_slice_ms);
    info!(
        "MQ: {}:{}",
        mq_config.host, mq_config.port
    );
    info!(
        "Redis: {}:{}",
        redis_config.host, redis_config.port
    );

    // 初始化 Redis 缓存
    let redis_cache = Arc::new(RedisCache::new(redis_config)?);

    // 初始化指标与健康检查
    let metrics = Arc::new(PushMetrics::new());
    let health_provider = Arc::new(PushHealthProvider::new(metrics.clone()));

    // 启动健康检查 HTTP 服务
    let health_addr = format!("{}:{}", push_config.listen_addr, push_config.listen_port + 100);
    let health_addr_clone = health_addr.clone();
    let hp = health_provider.clone();
    tokio::spawn(async move {
        let server = HealthServer::new(health_addr_clone, hp);
        if let Err(e) = server.run().await {
            error!(error = %e, "健康检查服务异常");
        }
    });
    info!("健康检查: http://{}/health", health_addr);
    info!("指标接口: http://{}/metrics", health_addr);

    // 尝试连接 Redis（失败不阻塞启动，后台重连）
    let rc = redis_cache.clone();
    tokio::spawn(async move {
        match rc.init().await {
            Ok(_) => info!("Redis 连接成功"),
            Err(e) => error!(error = %e, "Redis 连接失败，将在后续重连"),
        }
    });

    // 创建 WebSocket 服务器
    let ws_server = WebSocketServer::new(
        push_config.listen_string(),
        push_config.max_connections,
        push_config.heartbeat_interval_secs,
    );

    // 启动数据推送管道
    let pipeline = PushPipeline::new(
        ws_server.client_manager().clone(),
        redis_cache,
        mq_config,
    );
    pipeline.start_all().await?;

    // 启动 WebSocket 服务器
    let server_handle = tokio::spawn(async move {
        if let Err(e) = ws_server.run().await {
            error!(error = %e, "WebSocket 服务器运行出错");
        }
    });

    // 监听 Ctrl+C 信号
    tokio::signal::ctrl_c().await?;
    info!("收到停止信号，正在关闭推送服务...");

    // 终止服务器
    server_handle.abort();

    // 等待清理
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    info!("实时推送服务已停止");
    Ok(())
}

/// 初始化日志系统
fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,monitor_push=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}
