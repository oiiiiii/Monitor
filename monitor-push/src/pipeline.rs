//! 数据推送管道
//!
//! 从 MQ 消费实时数据，更新 Redis 缓存，
//! 并通过 WebSocket 推送给订阅的客户端。
//!
//! 推送优先级：
//! 1. 报警事件（最高优先级，立即推送）
//! 2. 生命体征（准实时）
//! 3. 波形数据（分片推送，200ms 一片）

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use lapin::{
    options::{BasicConsumeOptions, BasicQosOptions},
    types::FieldTable,
    Connection, ConnectionProperties,
};
use tracing::{error, info, warn};

use monitor_common::cache::RedisCache;
use monitor_common::mq::{MqConfig, QueueType};
use monitor_common::types::{AlarmEvent, VitalSignsData, WaveformData};

use crate::websocket::message::PushMessage;
use crate::websocket::session::ClientManager;

/// 数据推送管道
pub struct PushPipeline {
    client_manager: ClientManager,
    redis_cache: Arc<RedisCache>,
    mq_config: MqConfig,
}

impl PushPipeline {
    /// 创建新的推送管道
    pub fn new(
        client_manager: ClientManager,
        redis_cache: Arc<RedisCache>,
        mq_config: MqConfig,
    ) -> Self {
        Self {
            client_manager,
            redis_cache,
            mq_config,
        }
    }

    /// 启动所有数据推送任务
    pub async fn start_all(&self) -> anyhow::Result<()> {
        info!("启动数据推送管道...");

        // 生命体征推送
        let cm = self.client_manager.clone();
        let rc = self.redis_cache.clone();
        let mq = self.mq_config.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::run_vital_consumer(cm, rc, mq).await {
                error!(error = %e, "生命体征推送任务异常退出");
            }
        });

        // 波形推送
        let cm = self.client_manager.clone();
        let rc = self.redis_cache.clone();
        let mq = self.mq_config.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::run_wave_consumer(cm, rc, mq).await {
                error!(error = %e, "波形推送任务异常退出");
            }
        });

        // 报警推送（最高优先级）
        let cm = self.client_manager.clone();
        let rc = self.redis_cache.clone();
        let mq = self.mq_config.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::run_alarm_consumer(cm, rc, mq).await {
                error!(error = %e, "报警推送任务异常退出");
            }
        });

        info!("数据推送管道已启动");
        Ok(())
    }

    /// 生命体征消费者
    async fn run_vital_consumer(
        client_manager: ClientManager,
        redis_cache: Arc<RedisCache>,
        mq_config: MqConfig,
    ) -> anyhow::Result<()> {
        let addr = mq_config.amqp_url();
        let timeout = Duration::from_secs(mq_config.connection_timeout_secs);
        let conn = tokio::time::timeout(timeout, Connection::connect(&addr, ConnectionProperties::default()))
            .await
            .map_err(|e| anyhow::anyhow!("MQ 连接超时: {}", e))??;

        let channel = conn.create_channel().await?;
        channel
            .basic_qos(
                mq_config.prefetch_count,
                BasicQosOptions { global: false },
            )
            .await?;

        // 声明队列（确保存在）
        channel
            .queue_declare(
                QueueType::Vital.queue_name(),
                lapin::options::QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        let mut consumer = channel
            .basic_consume(
                QueueType::Vital.queue_name(),
                "push_vital_consumer",
                BasicConsumeOptions {
                    no_ack: true, // 推送服务可以自动 ACK，因为不影响数据持久化
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        info!("生命体征推送消费者启动");

        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    error!(error = %e, "消费生命体征消息失败");
                    continue;
                }
            };

            // 解析
            let vital: VitalSignsData = match serde_json::from_slice(&delivery.data) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "生命体征数据解析失败");
                    continue;
                }
            };

            let bed_no = vital.device.bed_no;

            // 更新 Redis 缓存
            let rc = redis_cache.clone();
            let vital_clone = vital.clone();
            tokio::spawn(async move {
                if let Err(e) = rc.update_vital_signs(&vital_clone).await {
                    warn!(error = %e, "更新生命体征缓存失败");
                }
            });

            // 推送给订阅的客户端
            let msg = PushMessage::vital_signs(&vital);
            client_manager.broadcast_to_bed(bed_no, msg);
        }

        warn!("生命体征推送消费者退出");
        Ok(())
    }

    /// 波形消费者
    async fn run_wave_consumer(
        client_manager: ClientManager,
        redis_cache: Arc<RedisCache>,
        mq_config: MqConfig,
    ) -> anyhow::Result<()> {
        let addr = mq_config.amqp_url();
        let timeout = Duration::from_secs(mq_config.connection_timeout_secs);
        let conn = tokio::time::timeout(timeout, Connection::connect(&addr, ConnectionProperties::default()))
            .await
            .map_err(|e| anyhow::anyhow!("MQ 连接超时: {}", e))??;

        let channel = conn.create_channel().await?;
        channel
            .basic_qos(
                mq_config.prefetch_count,
                BasicQosOptions { global: false },
            )
            .await?;

        // 声明队列
        channel
            .queue_declare(
                QueueType::Wave.queue_name(),
                lapin::options::QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        let mut consumer = channel
            .basic_consume(
                QueueType::Wave.queue_name(),
                "push_wave_consumer",
                BasicConsumeOptions {
                    no_ack: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        info!("波形推送消费者启动");

        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    error!(error = %e, "消费波形消息失败");
                    continue;
                }
            };

            // 解析（可能是单条或数组）
            let waves: Vec<WaveformData> = match serde_json::from_slice::<Vec<WaveformData>>(&delivery.data) {
                Ok(w) => w,
                Err(_) => {
                    // 尝试解析单条
                    match serde_json::from_slice::<WaveformData>(&delivery.data) {
                        Ok(w) => vec![w],
                        Err(e) => {
                            warn!(error = %e, "波形数据解析失败");
                            continue;
                        }
                    }
                }
            };

            if waves.is_empty() {
                continue;
            }

            let bed_no = waves[0].device.bed_no;

            // 更新 Redis 缓存（每条波形）
            let rc = redis_cache.clone();
            let waves_clone = waves.clone();
            tokio::spawn(async move {
                for wave in &waves_clone {
                    if let Err(e) = rc.append_waveform(wave).await {
                        warn!(error = %e, "更新波形缓存失败");
                        break;
                    }
                }
            });

            // 推送给订阅的客户端
            let msg = PushMessage::waveform(bed_no, &waves);
            client_manager.broadcast_to_bed(bed_no, msg);
        }

        warn!("波形推送消费者退出");
        Ok(())
    }

    /// 报警消费者（最高优先级）
    async fn run_alarm_consumer(
        client_manager: ClientManager,
        redis_cache: Arc<RedisCache>,
        mq_config: MqConfig,
    ) -> anyhow::Result<()> {
        let addr = mq_config.amqp_url();
        let timeout = Duration::from_secs(mq_config.connection_timeout_secs);
        let conn = tokio::time::timeout(timeout, Connection::connect(&addr, ConnectionProperties::default()))
            .await
            .map_err(|e| anyhow::anyhow!("MQ 连接超时: {}", e))??;

        let channel = conn.create_channel().await?;
        channel
            .basic_qos(
                mq_config.prefetch_count,
                BasicQosOptions { global: false },
            )
            .await?;

        // 声明队列
        channel
            .queue_declare(
                QueueType::Alarm.queue_name(),
                lapin::options::QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        let mut consumer = channel
            .basic_consume(
                QueueType::Alarm.queue_name(),
                "push_alarm_consumer",
                BasicConsumeOptions {
                    no_ack: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        info!("报警推送消费者启动");

        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    error!(error = %e, "消费报警消息失败");
                    continue;
                }
            };

            // 解析
            let alarm: AlarmEvent = match serde_json::from_slice(&delivery.data) {
                Ok(a) => a,
                Err(e) => {
                    warn!(error = %e, "报警数据解析失败");
                    continue;
                }
            };

            let bed_no = alarm.device.bed_no;

            info!(
                bed = bed_no,
                code = %alarm.alarm_code,
                priority = ?alarm.priority,
                phase = %alarm.event_phase,
                "收到报警，立即推送"
            );

            // 更新 Redis 缓存
            let rc = redis_cache.clone();
            let alarm_clone = alarm.clone();
            tokio::spawn(async move {
                if let Err(e) = rc.update_alarm(&alarm_clone).await {
                    warn!(error = %e, "更新报警缓存失败");
                }
            });

            // 立即推送给订阅的客户端（最高优先级）
            let msg = PushMessage::alarm(&alarm);
            client_manager.broadcast_to_bed(bed_no, msg);
        }

        warn!("报警推送消费者退出");
        Ok(())
    }
}
