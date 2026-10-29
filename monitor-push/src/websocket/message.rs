//! WebSocket 推送消息格式

use serde::{Deserialize, Serialize};

use monitor_common::types::{AlarmEvent, VitalSignsData, WaveformData};

/// 推送消息类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PushType {
    /// 生命体征更新
    VitalSigns,
    /// 波形数据
    Waveform,
    /// 报警事件
    Alarm,
    /// 设备状态变化
    DeviceStatus,
    /// 心跳
    Heartbeat,
    /// 订阅确认
    SubscribeAck,
    /// 错误
    Error,
}

/// 推送消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushMessage {
    /// 消息类型
    pub msg_type: PushType,
    /// 床位号
    pub bed_no: u16,
    /// 时间戳（毫秒）
    pub timestamp: i64,
    /// 消息数据（JSON）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl PushMessage {
    /// 创建生命体征消息
    pub fn vital_signs(vital: &VitalSignsData) -> Self {
        Self {
            msg_type: PushType::VitalSigns,
            bed_no: vital.device.bed_no,
            timestamp: vital.sample_time.timestamp_millis(),
            data: Some(serde_json::to_value(vital).unwrap_or_default()),
        }
    }

    /// 创建波形消息
    pub fn waveform(bed_no: u16, waves: &[WaveformData]) -> Self {
        let ts = waves.first().map(|w| w.start_time.timestamp_millis()).unwrap_or(0);
        Self {
            msg_type: PushType::Waveform,
            bed_no,
            timestamp: ts,
            data: Some(serde_json::to_value(waves).unwrap_or_default()),
        }
    }

    /// 创建报警消息
    pub fn alarm(alarm: &AlarmEvent) -> Self {
        Self {
            msg_type: PushType::Alarm,
            bed_no: alarm.device.bed_no,
            timestamp: alarm.alarm_time.timestamp_millis(),
            data: Some(serde_json::to_value(alarm).unwrap_or_default()),
        }
    }

    /// 创建心跳消息
    pub fn heartbeat() -> Self {
        Self {
            msg_type: PushType::Heartbeat,
            bed_no: 0,
            timestamp: chrono::Utc::now().timestamp_millis(),
            data: None,
        }
    }

    /// 创建订阅确认消息
    pub fn subscribe_ack(bed_no: u16, success: bool) -> Self {
        Self {
            msg_type: PushType::SubscribeAck,
            bed_no,
            timestamp: chrono::Utc::now().timestamp_millis(),
            data: Some(serde_json::json!({ "success": success })),
        }
    }

    /// 创建错误消息
    pub fn error(bed_no: u16, message: &str) -> Self {
        Self {
            msg_type: PushType::Error,
            bed_no,
            timestamp: chrono::Utc::now().timestamp_millis(),
            data: Some(serde_json::json!({ "message": message })),
        }
    }
}

/// 客户端请求消息
#[derive(Debug, Clone, Deserialize)]
pub struct ClientMessage {
    /// 请求类型
    pub action: ClientAction,
    /// 床位号
    pub bed_no: u16,
}

/// 客户端操作类型
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientAction {
    /// 订阅床位
    Subscribe,
    /// 取消订阅
    Unsubscribe,
    /// 心跳
    Ping,
    /// 请求历史数据
    RequestHistory,
}
