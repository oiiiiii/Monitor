//! 设备接入服务入口
//!
//! 启动 TCP 服务器，接收监护仪的 HL7 数据，
//! 解析后通过 MQ 发送给消费模块。
//! MQ 不可用时自动写入本地磁盘兜底缓存，
//! MQ 恢复后自动补发。

mod config;
mod connection;
mod device_manager;
mod event;
mod metrics;
mod pipeline;
mod server;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use monitor_common::buffer::DiskBufferConfig;
use monitor_common::mq::MqConfig;
use monitor_common::metrics::{AccessMetrics, health_server::HealthServer};

use config::AccessConfig;
use event::AccessEvent;
use metrics::AccessHealthProvider;
use pipeline::DataPipeline;
use server::AccessServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    init_logging();

    info!("========================================");
    info!("  多床位心电监护系统 - 设备接入服务");
    info!("========================================");

    // 加载配置
    let access_config = AccessConfig::load()?;
    let mq_config = MqConfig::from_env()?;
    let buffer_config = DiskBufferConfig::default();

    info!(
        "监听地址: {}:{}",
        access_config.listen_addr, access_config.listen_port
    );
    info!("连接超时: {}s", access_config.connection_timeout_secs);
    info!("最大连接数: {}", access_config.max_connections);
    info!("接收缓冲区: {} bytes", access_config.recv_buffer_size);
    info!("MQ 地址: {}:{}", mq_config.host, mq_config.port);
    info!("磁盘缓冲目录: {}", buffer_config.buffer_dir.display());

    // 初始化数据管道（MQ + 磁盘缓冲）
    let pipeline = DataPipeline::new(mq_config, buffer_config);
    pipeline.init().await?;

    // 初始化指标与健康检查
    let metrics = Arc::new(AccessMetrics::new());
    let health_provider = Arc::new(AccessHealthProvider::new(metrics.clone()));

    // 启动健康检查 HTTP 服务
    let health_addr = format!("{}:{}", access_config.listen_addr, access_config.listen_port + 100);
    let health_server = HealthServer::new(health_addr.clone(), health_provider.clone());
    tokio::spawn(async move {
        if let Err(e) = health_server.run().await {
            error!(error = %e, "健康检查服务异常");
        }
    });
    info!("健康检查: http://{}/health", health_addr);
    info!("指标接口: http://{}/metrics", health_addr);

    // 创建事件通道
    let (event_tx, mut event_rx) = mpsc::channel::<AccessEvent>(10000);

    // 启动事件处理任务
    let pipeline_clone = pipeline.clone();
    tokio::spawn(async move {
        info!("事件处理器启动");
        let mut vital_count: u64 = 0;
        let mut wave_count: u64 = 0;
        let mut alarm_count: u64 = 0;

        while let Some(event) = event_rx.recv().await {
            // 打印日志
            match &event {
                AccessEvent::DeviceConnected { device_ip, .. } => {
                    info!("[设备上线] IP: {}", device_ip);
                }
                AccessEvent::DeviceDisconnected { device_ip, reason } => {
                    info!("[设备下线] IP: {}, 原因: {}", device_ip, reason);
                }
                AccessEvent::VitalSignsReceived { device, data, .. } => {
                    vital_count += 1;
                    info!(
                        "[生命体征 #{vital_count}] IP: {}, HR: {:?}, SPO2: {:?}, RR: {:?}",
                        device.device_ip,
                        data.heart_rate,
                        data.spo2,
                        data.resp_rate,
                    );
                }
                AccessEvent::WaveformReceived { device, data, .. } => {
                    wave_count += 1;
                    info!(
                        "[波形数据 #{wave_count}] IP: {}, 导联数: {}",
                        device.device_ip,
                        data.len()
                    );
                }
                AccessEvent::AlarmReceived { device, data, .. } => {
                    alarm_count += 1;
                    info!(
                        "[报警事件 #{alarm_count}] IP: {}, 编码: {}, 优先级: {:?}",
                        device.device_ip,
                        data.alarm_code,
                        data.priority,
                    );
                }
                AccessEvent::ParseError { device_ip, error } => {
                    error!("[解析错误] IP: {}, 错误: {}", device_ip, error);
                }
            }

            // 发送到数据管道（MQ / 磁盘缓冲）
            pipeline_clone.process_event(&event).await;
        }
        info!("事件处理器退出");
    });

    // 启动服务器
    let server = AccessServer::new(access_config, event_tx);

    // 优雅停机监听
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            error!(error = %e, "服务器运行出错");
        }
    });

    // 监听 Ctrl+C 信号
    tokio::signal::ctrl_c().await?;
    info!("收到停止信号，正在关闭服务器...");

    // 给一点时间清理
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 终止服务器
    server_handle.abort();

    info!("设备接入服务已停止");
    Ok(())
}

/// 初始化日志系统
fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,monitor_access=debug,monitor_proto=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}
