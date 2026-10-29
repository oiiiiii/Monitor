//! 波形数据消费者
//!
//! 从 ecg_wave 队列消费波形数据，
//! 攒批后批量写入 TDengine，
//! 批量写入成功后统一 ACK，失败则 NACK（转入死信）。

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use lapin::options::{BasicAckOptions, BasicNackOptions};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use monitor_common::mq::{MqConfig, QueueType};
use monitor_common::types::WaveformData;

use super::base::BaseConsumer;
use crate::storage::tdengine::TdEngineStore;

/// 带 Delivery 的波形消息
struct WaveMessage {
    data: WaveformData,
    delivery_tag: u64,
}

/// 波形数据消费者
pub struct WaveConsumer {
    base: BaseConsumer,
    td_store: Arc<TdEngineStore>,
    batch_size: usize,
    batch_max_wait: Duration,
    max_retries: u32,
    retry_interval: Duration,
}

impl WaveConsumer {
    /// 创建新的波形消费者
    pub fn new(
        mq_config: MqConfig,
        td_store: Arc<TdEngineStore>,
        batch_size: usize,
        batch_max_wait_ms: u64,
        max_retries: u32,
        retry_interval_ms: u64,
    ) -> Self {
        Self {
            base: BaseConsumer::new(mq_config, QueueType::Wave),
            td_store,
            batch_size,
            batch_max_wait: Duration::from_millis(batch_max_wait_ms),
            max_retries,
            retry_interval: Duration::from_millis(retry_interval_ms),
        }
    }

    /// 启动消费循环
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut consumer = self.base.connect_and_consume().await?;
        let mut total_count: u64 = 0;

        info!("波形消费者启动，开始消费...");
        info!(
            batch_size = self.batch_size,
            batch_max_wait_ms = self.batch_max_wait.as_millis(),
            "批量写入配置"
        );

        // 攒批缓冲区
        let mut batch: Vec<WaveMessage> = Vec::with_capacity(self.batch_size);
        let mut batch_start = Instant::now();

        loop {
            // 计算下一批的超时时间
            let elapsed = batch_start.elapsed();
            let timeout = if batch.is_empty() {
                Duration::from_secs(60) // 空批次等待时间长一些
            } else {
                self.batch_max_wait.saturating_sub(elapsed)
            };

            // 带超时地接收消息
            let result = tokio::time::timeout(timeout, consumer.next()).await;

            match result {
                Ok(Some(Ok(delivery))) => {
                    // 解析消息
                    match serde_json::from_slice::<Vec<WaveformData>>(&delivery.data) {
                        Ok(waves) => {
                            let tag = delivery.delivery_tag;
                            // 每条波形单独存（但共享同一个 delivery tag 简化处理）
                            for wave in waves {
                                batch.push(WaveMessage {
                                    data: wave,
                                    delivery_tag: tag,
                                });
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "波形数据解析失败，NACK");
                            // 尝试直接解析单个 WaveformData
                            match serde_json::from_slice::<WaveformData>(&delivery.data) {
                                Ok(wave) => {
                                    let tag = delivery.delivery_tag;
                                    batch.push(WaveMessage {
                                        data: wave,
                                        delivery_tag: tag,
                                    });
                                }
                                Err(_) => {
                                    self.base.nack(&delivery).await;
                                }
                            }
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    error!(error = %e, "消费消息失败");
                    continue;
                }
                Ok(None) => {
                    // 消费者结束
                    warn!("波形消费者通道关闭");
                    break;
                }
                Err(_) => {
                    // 超时，检查是否需要刷批
                    if batch.is_empty() {
                        continue;
                    }
                }
            }

            // 检查是否达到刷批条件
            let should_flush = batch.len() >= self.batch_size
                || (!batch.is_empty() && batch_start.elapsed() >= self.batch_max_wait);

            if should_flush {
                let count = batch.len();

                // 提取所有波形数据
                let waveforms: Vec<WaveformData> = batch
                    .iter()
                    .map(|m| m.data.clone())
                    .collect();

                // 重试批量写入
                let mut success = false;
                for attempt in 0..=self.max_retries {
                    if attempt > 0 {
                        tokio::time::sleep(self.retry_interval).await;
                    }

                    match self.td_store.insert_waveforms(&waveforms).await {
                        Ok(_) => {
                            success = true;
                            break;
                        }
                        Err(e) => {
                            warn!(
                                attempt = attempt,
                                max = self.max_retries,
                                count = count,
                                error = %e,
                                "批量写入波形失败"
                            );
                        }
                    }
                }

                if success {
                    // 批量 ACK（这里简化为 ACK 最后一条消息的 tag，批量确认需要 multiple=true）
                    if let Some(last) = batch.last() {
                        // 使用 multiple ACK 确认这个 tag 之前的所有消息
                        if let Some(channel) = self.base.channel() {
                            let _ = channel
                                .basic_ack(last.delivery_tag, BasicAckOptions { multiple: true })
                                .await;
                        }
                    }

                    total_count += count as u64;

                    if total_count % 500 == 0 || count >= self.batch_size {
                        info!(count = total_count, batch_size = count, "已处理波形消息");
                    }
                } else {
                    // 批量 NACK
                    error!(count = count, "批量写入波形多次失败，NACK 转入死信");
                    if let Some(last) = batch.last() {
                        if let Some(channel) = self.base.channel() {
                            let _ = channel
                                .basic_nack(
                                    last.delivery_tag,
                                    BasicNackOptions {
                                        multiple: true,
                                        requeue: false,
                                    },
                                )
                                .await;
                        }
                    }
                }

                // 清空批次
                batch.clear();
                batch_start = Instant::now();
            }
        }

        warn!(total = total_count, "波形消费者退出");
        Ok(())
    }
}
