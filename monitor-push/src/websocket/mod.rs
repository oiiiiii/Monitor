//! WebSocket 推送模块
//!
//! 实现 WebSocket 实时推送服务：
//! - 按床位订阅机制
//! - 波形分片推送（200ms 数据量）
//! - 报警事件最高优先级立即推送
//! - 心跳保活

pub mod server;
pub mod session;
pub mod message;

pub use server::WebSocketServer;
pub use session::ClientSession;
pub use message::{PushMessage, PushType};
