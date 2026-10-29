//! 业务消费服务入口
//!
//! 从 RabbitMQ 消费三类数据，写入数据库：
//! - 生命体征 → PostgreSQL
//! - 波形数据 → TDengine（批量写入）
//! - 报警事件 → PostgreSQL
//!
//! 遵循项目规范：
//! - 手动 ACK，写入成功才确认
//! - 消费失败重试，超过次数转入死信
//! - 波形批量写入

mod config;
mod consumer;
mod storage;

use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use monitor_common::mq::MqConfig;

use config::ConsumerConfig;
use consumer::{AlarmConsumer, VitalConsumer, WaveConsumer};
use storage::{PostgresStore, TdEngineStore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    init_logging();

    info!("========================================");
    info!("  多床位心电监护系统 - 业务消费服务");
    info!("========================================");

    // 加载配置
    let consumer_config = ConsumerConfig::from_env()?;
    let mq_config = MqConfig::from_env()?;

    info!(
        "MQ 地址: {}:{}",
        mq_config.host, mq_config.port
    );
    info!(
        "TDengine: {}:{}",
        consumer_config.tdengine.host, consumer_config.tdengine.port
    );
    info!(
        "PostgreSQL: {}:{}",
        consumer_config.postgres.host, consumer_config.postgres.port
    );
    info!("预取数量: {}", mq_config.prefetch_count);
    info!("波形批量: {}", consumer_config.wave_batch_size);

    // 初始化存储
    info!("初始化存储...");

    let pg_store = Arc::new(PostgresStore::new(&consumer_config.postgres)?);
    pg_store.init().await?;
    info!("PostgreSQL 初始化完成");

    let td_store = Arc::new(TdEngineStore::new(consumer_config.tdengine.clone()));
    // TDengine 可能不可用时不阻塞启动
    let td_store_clone = td_store.clone();
    tokio::spawn(async move {
        match td_store_clone.init().await {
            Ok(_) => info!("TDengine 初始化完成"),
            Err(e) => error!(error = %e, "TDengine 初始化失败，将在后续重连"),
        }
    });

    // 启动消费者
    info!("启动消费者...");

    let mut handles = Vec::new();

    // 生命体征消费者
    let mq_config_clone = mq_config.clone();
    let pg_clone = pg_store.clone();
    let max_retries = consumer_config.max_retries;
    let retry_interval = consumer_config.retry_interval_ms;
    let vital_handle = tokio::spawn(async move {
        let mut consumer = VitalConsumer::new(
            mq_config_clone, pg_clone, max_retries, retry_interval);
        if let Err(e) = consumer.run().await {
            error!(error = %e, "生命体征消费者异常退出");
        }
    });
    handles.push(vital_handle);

    // 报警消费者
    let mq_config_clone = mq_config.clone();
    let pg_clone = pg_store.clone();
    let alarm_handle = tokio::spawn(async move {
        let mut consumer = AlarmConsumer::new(
            mq_config_clone, pg_clone, max_retries, retry_interval);
        if let Err(e) = consumer.run().await {
            error!(error = %e, "报警消费者异常退出");
        }
    });
    handles.push(alarm_handle);

    // 波形消费者
    let mq_config_clone = mq_config.clone();
    let td_clone = td_store.clone();
    let wave_batch_size = consumer_config.wave_batch_size;
    let wave_batch_max_wait = consumer_config.wave_batch_max_wait_ms;
    let wave_handle = tokio::spawn(async move {
        let mut consumer = WaveConsumer::new(
            mq_config_clone,
            td_clone,
            wave_batch_size,
            wave_batch_max_wait,
            max_retries,
            retry_interval,
        );
        if let Err(e) = consumer.run().await {
            error!(error = %e, "波形消费者异常退出");
        }
    });
    handles.push(wave_handle);

    info!("所有消费者已启动");

    // 监听 Ctrl+C 信号
    tokio::signal::ctrl_c().await?;
    info!("收到停止信号，正在关闭消费者...");

    // 终止所有消费者
    for handle in handles {
        handle.abort();
    }

    // 等待一小段时间让任务清理
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    info!("业务消费服务已停止");
    Ok(())
}

/// 初始化日志系统
fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,monitor_consumer=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}
