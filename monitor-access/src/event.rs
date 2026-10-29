//! 设备接入事件定义
//!
//! 定义设备上线、下线、数据接收等事件，
//! 用于设备接入层与上层消费模块之间的通信。

use std::time::Instant;

use monitor_common::types::{AlarmEvent, DeviceInfo, VitalSignsData, WaveformData};

/// 设备接入事件
#[derive(Debug, Clone)]
pub enum AccessEvent {
    /// 设备上线
    DeviceConnected {
        /// 设备IP
        device_ip: String,
        /// 连接时间
        connect_time: Instant,
    },

    /// 设备断开
    DeviceDisconnected {
        /// 设备IP
        device_ip: String,
        /// 断开原因
        reason: DisconnectReason,
    },

    /// 收到生命体征数据
    VitalSignsReceived {
        /// 设备信息
        device: DeviceInfo,
        /// 生命体征数据
        data: Box<VitalSignsData>,
        /// 接收时间
        recv_time: Instant,
    },

    /// 收到波形数据
    WaveformReceived {
        /// 设备信息
        device: DeviceInfo,
        /// 波形数据列表
        data: Vec<WaveformData>,
        /// 接收时间
        recv_time: Instant,
    },

    /// 收到报警事件
    AlarmReceived {
        /// 设备信息
        device: DeviceInfo,
        /// 报警事件
        data: Box<AlarmEvent>,
        /// 接收时间
        recv_time: Instant,
    },

    /// 解析错误
    ParseError {
        /// 设备IP
        device_ip: String,
        /// 错误信息
        error: String,
    },
}

/// 断开连接原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    /// 正常断开
    Normal,
    /// 连接超时
    Timeout,
    /// 协议错误
    ProtocolError,
    /// 网络错误
    NetworkError,
    /// 服务器主动断开
    ServerInitiated,
    /// 未知原因
    Unknown,
}

impl DisconnectReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            DisconnectReason::Normal => "正常断开",
            DisconnectReason::Timeout => "连接超时",
            DisconnectReason::ProtocolError => "协议错误",
            DisconnectReason::NetworkError => "网络错误",
            DisconnectReason::ServerInitiated => "服务器主动断开",
            DisconnectReason::Unknown => "未知原因",
        }
    }
}

impl std::fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
