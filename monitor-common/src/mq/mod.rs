//! RabbitMQ 消息队列模块
//!
//! 实现消息缓冲层，符合项目架构规范：
//! - 4 个独立队列：ecg_wave、ecg_vital、ecg_alarm、ecg_raw
//! - 队列与消息持久化
//! - Publisher Confirm 生产者确认
//! - 死信队列
//! - 手动 ACK（消费者侧）

pub mod config;
pub mod queues;
pub mod producer;

pub use config::MqConfig;
pub use queues::QueueType;
pub use producer::MqProducer;
