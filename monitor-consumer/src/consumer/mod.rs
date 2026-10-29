//! 消费者模块
//!
//! 从 RabbitMQ 消费数据，经过清洗后写入数据库。
//! 遵循项目规范：手动 ACK、写入成功才确认、消费失败转入死信。

pub mod base;
pub mod vital_consumer;
pub mod wave_consumer;
pub mod alarm_consumer;

pub use base::BaseConsumer;
pub use vital_consumer::VitalConsumer;
pub use wave_consumer::WaveConsumer;
pub use alarm_consumer::AlarmConsumer;
