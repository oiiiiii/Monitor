//! 数据管道：接入层 -> MQ / 本地缓冲
//!
//! 负责将接入层解析后的数据发送到 RabbitMQ，
//! MQ 不可用时写入本地磁盘兜底缓存，
//! MQ 恢复后自动补发。

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use monitor_common::buffer::{BufferedMessage, DiskBuffer, DiskBufferConfig};
use monitor_common::mq::{MqConfig, MqProducer, QueueType};
use monitor_common::types::{AlarmEvent, VitalSignsData, WaveformData};

use crate::event::AccessEvent;

/// 数据管道
#[derive(Clone)]
pub struct DataPipeline {
    producer: MqProducer,
    disk_buffer: Arc<DiskBuffer>,
    /// 是否启用磁盘缓冲（MQ 不可用时自动启用）
    fallback_enabled: bool,
}

impl DataPipeline {
    /// 创建新的数据管道
    pub fn new(mq_config: MqConfig, buffer_config: DiskBufferConfig) -> Self {
        let producer = MqProducer::new(mq_config);
        let disk_buffer = Arc::new(DiskBuffer::new(buffer_config));

        Self {
            producer,
            disk_buffer,
            fallback_enabled: true,
        }
    }

    /// 获取磁盘缓冲引用
    pub fn disk_buffer(&self) -> Arc<DiskBuffer> {
        self.disk_buffer.clone()
    }

    /// 获取生产者引用
    pub fn producer(&self) -> &MqProducer {
        &self.producer
    }

    /// 初始化（连接 MQ，初始化磁盘缓冲）
    pub async fn init(&self) -> anyhow::Result<()> {
        // 初始化磁盘缓冲
        self.disk_buffer.init().await?;

        // 尝试连接 MQ（失败不阻塞，启动后后台重连）
        match self.producer.connect().await {
            Ok(_) => {
                info!("MQ 连接成功");

                // 检查是否有积压的缓存数据需要补发
                self.start_replay_task();
            }
            Err(e) => {
                warn!(error = %e, "MQ 连接失败，将使用本地磁盘缓冲，后台自动重连");
                // 启动后台重连任务
                self.start_reconnect_task();
            }
        }

        Ok(())
    }

    /// 处理接入事件并发送到 MQ
    pub async fn process_event(&self, event: &AccessEvent) {
        match event {
            AccessEvent::VitalSignsReceived { device, data, .. } => {
                self.send_vital_signs(device.device_ip.as_str(), data)
                    .await;
            }
            AccessEvent::WaveformReceived { device, data, .. } => {
                self.send_waveform(device.device_ip.as_str(), data).await;
            }
            AccessEvent::AlarmReceived { device, data, .. } => {
                self.send_alarm(device.device_ip.as_str(), data).await;
            }
            _ => {
                // 其他事件不发送到 MQ
            }
        }
    }

    /// 发送生命体征数据
    async fn send_vital_signs(&self, device_ip: &str, data: &VitalSignsData) {
        let queue = QueueType::Vital;
        let payload = match serde_json::to_vec(data) {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "序列化生命体征数据失败");
                return;
            }
        };

        self.send_with_fallback(queue, device_ip, payload).await;
    }

    /// 发送波形数据
    async fn send_waveform(&self, device_ip: &str, data: &[WaveformData]) {
        let queue = QueueType::Wave;
        let payload = match serde_json::to_vec(data) {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "序列化波形数据失败");
                return;
            }
        };

        self.send_with_fallback(queue, device_ip, payload).await;
    }

    /// 发送报警事件
    async fn send_alarm(&self, device_ip: &str, data: &AlarmEvent) {
        let queue = QueueType::Alarm;
        let payload = match serde_json::to_vec(data) {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "序列化报警数据失败");
                return;
            }
        };

        self.send_with_fallback(queue, device_ip, payload).await;
    }

    /// 发送消息，失败则写入磁盘缓冲
    async fn send_with_fallback(&self, queue: QueueType, device_ip: &str, payload: Vec<u8>) {
        // 先尝试发送到 MQ
        if self.producer.is_connected().await {
            match self.producer.publish(queue, &payload).await {
                Ok(_) => {
                    return;
                }
                Err(e) => {
                    warn!(
                        queue = queue.queue_name(),
                        error = %e,
                        "发送到 MQ 失败，写入磁盘缓冲"
                    );
                }
            }
        }

        // MQ 不可用或发送失败，写入磁盘缓冲
        if self.fallback_enabled {
            let msg = BufferedMessage::new(queue, device_ip, payload);
            if let Err(e) = self.disk_buffer.write(&msg).await {
                error!(
                    queue = queue.queue_name(),
                    error = %e,
                    "写入磁盘缓冲失败！数据可能丢失！"
                );
            }
        }
    }

    /// 启动后台重连任务
    fn start_reconnect_task(&self) {
        let producer = self.producer.clone();
        let disk_buffer = self.disk_buffer.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;

                if producer.is_connected().await {
                    continue;
                }

                info!("尝试重连 MQ...");
                match producer.connect().await {
                    Ok(_) => {
                        info!("MQ 重连成功");

                        // 重连成功，开始补发缓存数据
                        Self::replay_buffer(&producer, &disk_buffer).await;
                    }
                    Err(e) => {
                        warn!(error = %e, "MQ 重连失败，稍后重试");
                    }
                }
            }
        });
    }

    /// 启动补发任务
    fn start_replay_task(&self) {
        let producer = self.producer.clone();
        let disk_buffer = self.disk_buffer.clone();

        tokio::spawn(async move {
            // 启动时检查一次
            tokio::time::sleep(Duration::from_secs(2)).await;
            Self::replay_buffer(&producer, &disk_buffer).await;
        });
    }

    /// 补发磁盘缓冲中的数据
    async fn replay_buffer(producer: &MqProducer, disk_buffer: &Arc<DiskBuffer>) {
        info!("开始补发磁盘缓冲数据...");

        let messages = match disk_buffer.drain_all().await {
            Ok(msgs) => msgs,
            Err(e) => {
                error!(error = %e, "读取磁盘缓冲失败");
                return;
            }
        };

        if messages.is_empty() {
            info!("磁盘缓冲为空，无需补发");
            return;
        }

        let total = messages.len();
        info!(count = total, "读取到待补发消息");

        let mut success = 0u64;
        let mut failed = 0u64;

        for msg in messages {
            let queue = match msg.queue.as_str() {
                "ecg_wave" => QueueType::Wave,
                "ecg_vital" => QueueType::Vital,
                "ecg_alarm" => QueueType::Alarm,
                "ecg_raw" => QueueType::Raw,
                _ => {
                    warn!(queue = %msg.queue, "未知队列类型，跳过");
                    failed += 1;
                    continue;
                }
            };

            match producer.publish(queue, &msg.payload).await {
                Ok(_) => {
                    success += 1;
                }
                Err(e) => {
                    warn!(
                        queue = msg.queue.as_str(),
                        error = %e,
                        "补发消息失败"
                    );
                    failed += 1;
                }
            }
        }

        info!(
            total = total,
            success = success,
            failed = failed,
            "磁盘缓冲补发完成"
        );
    }
}
