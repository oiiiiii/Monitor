//! Redis 缓存模块
//!
//! 实现热数据缓存，符合项目规范：
//! - 最新体征 Hash 缓存
//! - 最近 10 秒波形 List 缓存（固定长度环形列表）
//! - 活跃报警 Set 缓存
//! - 设备在线状态 Hash

pub mod redis_cache;

pub use redis_cache::{RedisCache, RedisConfig};
