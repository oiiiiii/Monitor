//! RabbitMQ 生产者
//!
//! 实现可靠的消息投递：
//! - Publisher Confirm 机制
//! - 消息持久化
//! - 失败重试
//! - 连接断开自动重连

use std::sync::Arc;
use std::time::Duration;

use lapin::{
    options::{BasicPublishOptions, ConfirmSelectOptions},
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable, LongString, ShortString},
    BasicProperties, Channel, Connection, ConnectionProperties,
};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use super::config::MqConfig;
use super::queues::{declare_all, QueueType};

/// RabbitMQ 生产者
#[derive(Clone)]
pub struct MqProducer {
    inner: Arc<Inner>,
}

struct Inner {
    config: MqConfig,
    channel: Mutex<Option<Channel>>,
}

impl MqProducer {
    /// 创建新的生产者
    pub fn new(config: MqConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                channel: Mutex::new(None),
            }),
        }
    }

    /// 连接并初始化（声明交换机和队列）
    pub async fn connect(&self) -> anyhow::Result<()> {
        let mut channel_guard = self.inner.channel.lock().await;

        info!(
            host = %self.inner.config.host,
            port = self.inner.config.port,
            "连接 RabbitMQ"
        );

        // 创建连接
        let addr = self.inner.config.amqp_url();
        let timeout = Duration::from_secs(self.inner.config.connection_timeout_secs);
        let conn = tokio::time::timeout(timeout, Connection::connect(&addr, ConnectionProperties::default()))
            .await
            .map_err(|e| anyhow::anyhow!("MQ 连接超时: {}", e))??;

        info!("RabbitMQ 连接成功");

        // 创建通道
        let channel = conn.create_channel().await?;

        // 启用 Publisher Confirm
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await?;

        info!("Publisher Confirm 已启用");

        // 声明交换机和队列
        declare_all(&channel, &self.inner.config.exchange).await?;

        *channel_guard = Some(channel);

        info!("RabbitMQ 生产者初始化完成");
        Ok(())
    }

    /// 检查是否已连接
    pub async fn is_connected(&self) -> bool {
        let channel_guard = self.inner.channel.lock().await;
        channel_guard.is_some()
    }

    /// 发送消息到指定队列
    ///
    /// 包含 Publisher Confirm 确认和重试机制
    pub async fn publish(
        &self,
        queue_type: QueueType,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        let max_retries = self.inner.config.max_retries;
        let retry_interval = Duration::from_millis(self.inner.config.retry_interval_ms);
        let confirm_timeout = Duration::from_millis(self.inner.config.confirm_timeout_ms);

        let mut last_error = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                warn!(
                    queue = queue_type.queue_name(),
                    attempt = attempt,
                    max = max_retries,
                    "重试发送消息"
                );
                tokio::time::sleep(retry_interval).await;
            }

            // 确保连接可用
            if let Err(e) = self.ensure_connected().await {
                last_error = Some(e);
                continue;
            }

            // 发送消息
            {
                let channel_guard = self.inner.channel.lock().await;
                let channel = match channel_guard.as_ref() {
                    Some(ch) => ch,
                    None => {
                        last_error = Some(anyhow::anyhow!("通道未初始化"));
                        continue;
                    }
                };

                let routing_key = queue_type.routing_key();
                let exchange = &self.inner.config.exchange;

                // 持久化消息属性
                let props = BasicProperties::default()
                    .with_delivery_mode(2); // 2 = 持久化

                // 发送
                let confirm = channel
                    .basic_publish(
                        exchange,
                        routing_key,
                        BasicPublishOptions {
                            mandatory: true,
                            ..Default::default()
                        },
                        payload,
                        props,
                    )
                    .await?;

                // 等待确认
                let result = tokio::time::timeout(confirm_timeout, confirm).await;

                match result {
                    Ok(Ok(Confirmation::Ack(_))) => {
                        // 发送成功
                        return Ok(());
                    }
                    Ok(Ok(Confirmation::Nack(_))) => {
                        last_error = Some(anyhow::anyhow!("消息被 Broker Nack"));
                        continue;
                    }
                    Ok(Ok(Confirmation::NotRequested)) => {
                        // Confirm 未启用，但消息应该已经发出去了
                        return Ok(());
                    }
                    Ok(Err(e)) => {
                        last_error = Some(anyhow::Error::from(e));
                        continue;
                    }
                    Err(_) => {
                        last_error = Some(anyhow::anyhow!("等待 Publisher Confirm 超时"));
                        continue;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("未知错误")))
    }

    /// 确保连接可用，如果断开则重连
    async fn ensure_connected(&self) -> anyhow::Result<()> {
        {
            let channel_guard = self.inner.channel.lock().await;
            if let Some(channel) = channel_guard.as_ref() {
                if channel.status().connected() {
                    return Ok(());
                }
            }
        }

        // 需要重连
        warn!("RabbitMQ 连接断开，尝试重连...");
        self.connect().await
    }
}
