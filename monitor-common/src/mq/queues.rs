//! 队列定义
//!
//! 按项目规范定义 4 个业务队列 + 1 个死信队列：
//! - ecg_wave: 波形数据
//! - ecg_vital: 生命体征数据
//! - ecg_alarm: 报警事件
//! - ecg_raw: 原始报文
//! - ecg_dead_letter: 死信队列

use lapin::{
    options::{ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
    types::{AMQPValue, FieldTable, LongString, ShortString},
    ExchangeKind,
};
use tracing::info;

/// 队列类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueType {
    /// 波形数据队列
    Wave,
    /// 生命体征队列
    Vital,
    /// 报警事件队列
    Alarm,
    /// 原始报文队列
    Raw,
}

impl QueueType {
    /// 获取队列名称
    pub fn queue_name(&self) -> &'static str {
        match self {
            QueueType::Wave => "ecg_wave",
            QueueType::Vital => "ecg_vital",
            QueueType::Alarm => "ecg_alarm",
            QueueType::Raw => "ecg_raw",
        }
    }

    /// 获取路由键
    pub fn routing_key(&self) -> &'static str {
        match self {
            QueueType::Wave => "ecg.wave",
            QueueType::Vital => "ecg.vital",
            QueueType::Alarm => "ecg.alarm",
            QueueType::Raw => "ecg.raw",
        }
    }

    /// 获取死信队列名称
    pub fn dead_letter_queue_name() -> &'static str {
        "ecg_dead_letter"
    }

    /// 获取死信交换机名称
    pub fn dead_letter_exchange_name() -> &'static str {
        "ecg_dead_letter_exchange"
    }

    /// 所有队列类型
    pub fn all() -> [QueueType; 4] {
        [
            QueueType::Wave,
            QueueType::Vital,
            QueueType::Alarm,
            QueueType::Raw,
        ]
    }
}

/// 声明所有队列和交换机
///
/// 包括：
/// - 主交换机（topic 类型）
/// - 死信交换机（direct 类型）
/// - 4 个业务队列（持久化，绑定死信交换机）
/// - 1 个死信队列
pub async fn declare_all(
    channel: &lapin::Channel,
    exchange_name: &str,
) -> anyhow::Result<()> {
    // 1. 声明死信交换机
    let dlx_name = QueueType::dead_letter_exchange_name();
    channel
        .exchange_declare(
            dlx_name,
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                auto_delete: false,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    info!(exchange = dlx_name, "声明死信交换机");

    // 2. 声明死信队列
    let dlq_name = QueueType::dead_letter_queue_name();
    channel
        .queue_declare(
            dlq_name,
            QueueDeclareOptions {
                durable: true,
                exclusive: false,
                auto_delete: false,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    // 绑定死信队列到死信交换机
    channel
        .queue_bind(
            dlq_name,
            dlx_name,
            "#", // 匹配所有路由键
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    info!(queue = dlq_name, "声明死信队列");

    // 3. 声明主交换机（topic 类型）
    channel
        .exchange_declare(
            exchange_name,
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                durable: true,
                auto_delete: false,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    info!(exchange = exchange_name, "声明主交换机");

    // 4. 声明 4 个业务队列
    for queue_type in QueueType::all() {
        let queue_name = queue_type.queue_name();
        let routing_key = queue_type.routing_key();

        // 队列参数：配置死信交换机
        let mut arguments = FieldTable::default();
        arguments.insert(
            ShortString::from("x-dead-letter-exchange"),
            AMQPValue::LongString(LongString::from(dlx_name)),
        );

        channel
            .queue_declare(
                queue_name,
                QueueDeclareOptions {
                    durable: true,
                    exclusive: false,
                    auto_delete: false,
                    ..Default::default()
                },
                arguments,
            )
            .await?;

        // 绑定队列到交换机
        channel
            .queue_bind(
                queue_name,
                exchange_name,
                routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;

        info!(queue = queue_name, routing_key = routing_key, "声明业务队列");
    }

    Ok(())
}
