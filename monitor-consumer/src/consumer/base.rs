//! 消费者基类
//!
//! 封装通用的消费逻辑：
//! - 连接 MQ 连接与消费
//! - 手动 ACK / NACK
//! - 消费失败重试
//! - 死信队列转移

use std::time::Duration;

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions, QueueDeclareOptions},
    types::FieldTable,
    Channel, Connection, ConnectionProperties,
};
use tracing::{error, info, warn};

use monitor_common::mq::{MqConfig, QueueType};

/// 消费者基类
pub struct BaseConsumer {
    mq_config: MqConfig,
    channel: Option<Channel>,
    queue_type: QueueType,
    consumer_tag: String,
}

impl BaseConsumer {
    /// 创建新的消费者
    pub fn new(mq_config: MqConfig, queue_type: QueueType) -> Self {
        let consumer_tag = format!(
            "{}_{}",
            mq_config.exchange,
            queue_type.queue_name()
        );
        Self {
            mq_config,
            channel: None,
            queue_type,
            consumer_tag,
        }
    }

    /// 获取队列类型
    pub fn queue_type(&self) -> QueueType {
        self.queue_type
    }

    /// 连接并开始消费
    pub async fn connect_and_consume(&mut self) -> anyhow::Result<lapin::Consumer> {
        let addr = self.mq_config.amqp_url();
        let timeout = Duration::from_secs(self.mq_config.connection_timeout_secs);
        let conn = tokio::time::timeout(timeout, Connection::connect(&addr, ConnectionProperties::default()))
            .await
            .map_err(|e| anyhow::anyhow!("MQ 连接超时: {}", e))??;

        info!(
            queue = self.queue_type.queue_name(),
            "消费者连接 MQ 成功"
        );

        let channel = conn.create_channel().await?;

        // 设置 QoS（预取数量）
        channel
            .basic_qos(
            self.mq_config.prefetch_count,
            BasicQosOptions { global: false },
        )
        .await?;

        // 声明队列（确保队列存在）
        channel
            .queue_declare(
                self.queue_type.queue_name(),
                QueueDeclareOptions {
                    durable: true,
                    exclusive: false,
                    auto_delete: false,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        // 开始消费
        let consumer = channel
            .basic_consume(
                self.queue_type.queue_name(),
                &self.consumer_tag,
                BasicConsumeOptions {
                    no_ack: false, // 手动 ACK
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        self.channel = Some(channel);

        info!(
            queue = self.queue_type.queue_name(),
            prefetch = self.mq_config.prefetch_count,
            "消费者启动成功"
        );

        Ok(consumer)
    }

    /// 确认消息（ACK）
    pub async fn ack(&self, delivery: &lapin::message::Delivery) {
        if let Some(channel) = &self.channel {
            if let Err(e) = delivery
                .ack(BasicAckOptions::default())
                .await
            {
                error!(error = %e, "ACK 失败");
            }
        }
    }

    /// 拒绝消息（NACK，不重新入队，会转入死信队列）
    pub async fn nack(&self, delivery: &lapin::message::Delivery) {
        if let Some(channel) = &self.channel {
            if let Err(e) = delivery
                .nack(BasicNackOptions {
                    requeue: false, // 不重新入队，转入死信
                    multiple: false,
                })
                .await
            {
                error!(error = %e, "NACK 失败");
            }
        }
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        self.channel.is_some()
    }

    /// 获取 Channel 引用
    pub fn channel(&self) -> Option<&Channel> {
        self.channel.as_ref()
    }

    /// 获取 MQ 配置
    pub fn mq_config(&self) -> &MqConfig {
        &self.mq_config
    }
}
