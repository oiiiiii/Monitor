//! 进程内消息总线（替代 RabbitMQ）
//!
//! 使用 tokio::sync::broadcast 实现多生产者多消费者模型
//! - 接入层生产消息
//! - 存储和推送服务消费消息

use std::sync::Arc;

use anyhow::Result;
use monitor_common::types::{AlarmEvent, VitalSignsData, WaveformData};
use tokio::sync::broadcast;
use tracing::{debug, warn};

/// 消息总线
#[derive(Clone)]
pub struct MessageBus {
    vital_tx: broadcast::Sender<VitalSignsData>,
    waveform_tx: broadcast::Sender<WaveformData>,
    alarm_tx: broadcast::Sender<AlarmEvent>,
}

impl MessageBus {
    /// 创建消息总线
    pub fn new(capacity: usize) -> Self {
        let (vital_tx, _) = broadcast::channel(capacity);
        let (waveform_tx, _) = broadcast::channel(capacity);
        let (alarm_tx, _) = broadcast::channel(capacity);

        Self {
            vital_tx,
            waveform_tx,
            alarm_tx,
        }
    }

    /// 发布体征数据
    pub fn publish_vital(&self, vital: VitalSignsData) -> Result<()> {
        match self.vital_tx.send(vital) {
            Ok(count) => {
                debug!("体征消息已发布，订阅者数: {}", count);
                Ok(())
            }
            Err(e) => {
                warn!("体征消息发送失败: {:?}", e);
                Ok(())
            }
        }
    }

    /// 发布波形数据
    pub fn publish_waveform(&self, wave: WaveformData) -> Result<()> {
        match self.waveform_tx.send(wave) {
            Ok(count) => {
                debug!("波形消息已发布，订阅者数: {}", count);
                Ok(())
            }
            Err(e) => {
                warn!("波形消息发送失败: {:?}", e);
                Ok(())
            }
        }
    }

    /// 发布报警事件
    pub fn publish_alarm(&self, alarm: AlarmEvent) -> Result<()> {
        match self.alarm_tx.send(alarm) {
            Ok(count) => {
                debug!("报警消息已发布，订阅者数: {}", count);
                Ok(())
            }
            Err(e) => {
                warn!("报警消息发送失败: {:?}", e);
                Ok(())
            }
        }
    }

    /// 订阅体征数据
    pub fn subscribe_vital(&self) -> broadcast::Receiver<VitalSignsData> {
        self.vital_tx.subscribe()
    }

    /// 订阅波形数据
    pub fn subscribe_waveform(&self) -> broadcast::Receiver<WaveformData> {
        self.waveform_tx.subscribe()
    }

    /// 订阅报警事件
    pub fn subscribe_alarm(&self) -> broadcast::Receiver<AlarmEvent> {
        self.alarm_tx.subscribe()
    }

    }

/// 共享消息总线
pub type SharedMessageBus = Arc<MessageBus>;
