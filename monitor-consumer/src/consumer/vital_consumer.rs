//! 生命体征消费者
//!
//! 从 ecg_vital 队列消费生命体征数据，
//! 经过清洗后写入 PostgreSQL，
//! 写入成功后手动 ACK，失败则 NACK（转入死信）。

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{error, info, warn};

use monitor_common::mq::{MqConfig, QueueType};
use monitor_common::types::VitalSignsData;

use super::base::BaseConsumer;
use crate::storage::postgres::PostgresStore;

/// 生命体征消费者
pub struct VitalConsumer {
    base: BaseConsumer,
    pg_store: Arc<PostgresStore>,
    max_retries: u32,
    retry_interval: Duration,
}

impl VitalConsumer {
    /// 创建新的生命体征消费者
    pub fn new(
        mq_config: MqConfig,
        pg_store: Arc<PostgresStore>,
        max_retries: u32,
        retry_interval_ms: u64,
    ) -> Self {
        Self {
            base: BaseConsumer::new(mq_config, QueueType::Vital),
            pg_store,
            max_retries,
            retry_interval: Duration::from_millis(retry_interval_ms),
        }
    }

    /// 启动消费循环
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut consumer = self.base.connect_and_consume().await?;
        let mut count: u64 = 0;

        info!("生命体征消费者启动，开始消费...");

        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    error!(error = %e, "消费消息失败");
                    continue;
                }
            };

            // 解析消息
            let vital: VitalSignsData = match serde_json::from_slice(&delivery.data) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "生命体征数据解析失败，NACK");
                    self.base.nack(&delivery).await;
                    continue;
                }
            };

            // 重试写入数据库
            let mut success = false;
            for attempt in 0..=self.max_retries {
                if attempt > 0 {
                    tokio::time::sleep(self.retry_interval).await;
                }

                match self.pg_store.insert_vital_signs(&vital).await {
                    Ok(_) => {
                        success = true;
                        break;
                    }
                    Err(e) => {
                        warn!(
                            attempt = attempt,
                            max = self.max_retries,
                            error = %e,
                            "写入生命体征失败"
                        );
                    }
                }
            }

            if success {
                // 写入成功，ACK
                self.base.ack(&delivery).await;
                count += 1;

                if count % 100 == 0 {
                    info!(count = count, "已处理生命体征消息");
                }
            } else {
                // 多次失败，NACK（转入死信）
                error!("写入生命体征多次失败，NACK 转入死信");
                self.base.nack(&delivery).await;
            }
        }

        warn!("生命体征消费者退出");
        Ok(())
    }
}
