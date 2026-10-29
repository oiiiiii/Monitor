//! 公共数据类型定义

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 设备厂商枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceVendor {
    /// 科曼
    Comen,
    /// 迈瑞
    Mindray,
}

impl fmt::Display for DeviceVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceVendor::Comen => write!(f, "comen"),
            DeviceVendor::Mindray => write!(f, "mindray"),
        }
    }
}

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// 设备 IP
    pub device_ip: String,
    /// 设备厂商
    pub vendor: DeviceVendor,
    /// 设备型号
    pub model: String,
    /// 设备序列号
    pub serial: String,
    /// 关联床位号
    pub bed_no: u16,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            device_ip: String::new(),
            vendor: DeviceVendor::Mindray,
            model: String::new(),
            serial: String::new(),
            bed_no: 0,
        }
    }
}

/// 数据类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    /// 生命体征
    VitalSigns,
    /// 心电波形
    Waveform,
    /// 报警事件
    Alarm,
}

/// 导联枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LeadType {
    #[serde(rename = "I")]
    I,
    #[serde(rename = "II")]
    II,
    #[serde(rename = "III")]
    III,
    #[serde(rename = "aVR")]
    AVR,
    #[serde(rename = "aVL")]
    AVL,
    #[serde(rename = "aVF")]
    AVF,
    #[serde(rename = "V1")]
    V1,
    #[serde(rename = "V2")]
    V2,
    #[serde(rename = "V3")]
    V3,
    #[serde(rename = "V4")]
    V4,
    #[serde(rename = "V5")]
    V5,
    #[serde(rename = "V6")]
    V6,
    #[serde(rename = "IBP")]
    IBP,
    #[serde(rename = "SpO2")]
    SPO2,
    #[serde(rename = "Resp")]
    RESP,
}

impl LeadType {
    /// 从字符串解析导联类型
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "I" | "MDC_ECG_ELEC_POTL_I" | "131329" => Some(LeadType::I),
            "II" | "MDC_ECG_ELEC_POTL_II" | "131330" => Some(LeadType::II),
            "III" | "MDC_ECG_ELEC_POTL_III" | "131389" => Some(LeadType::III),
            "AVR" | "MDC_ECG_ELEC_POTL_AVR" | "131390" => Some(LeadType::AVR),
            "AVL" | "MDC_ECG_ELEC_POTL_AVL" | "131391" => Some(LeadType::AVL),
            "AVF" | "MDC_ECG_ELEC_POTL_AVF" | "131392" => Some(LeadType::AVF),
            "V1" | "MDC_ECG_ELEC_POTL_V1" | "131396" => Some(LeadType::V1),
            "V2" | "MDC_ECG_ELEC_POTL_V2" | "131397" => Some(LeadType::V2),
            "V3" | "MDC_ECG_ELEC_POTL_V3" | "131398" => Some(LeadType::V3),
            "V4" | "MDC_ECG_ELEC_POTL_V4" | "131399" => Some(LeadType::V4),
            "V5" | "MDC_ECG_ELEC_POTL_V5" | "131400" => Some(LeadType::V5),
            "V6" | "MDC_ECG_ELEC_POTL_V6" | "131401" => Some(LeadType::V6),
            "V" | "MDC_ECG_ELEC_POTL_V" | "131395" => Some(LeadType::V1),
            "IBP" | "MDC_PRESS_BLD_ART" | "150032" => Some(LeadType::IBP),
            "SPO2" | "MDC_PULS_OXIM_PLETH" | "150452" => Some(LeadType::SPO2),
            "RESP" | "MDC_IMPED_TTHOR" | "151780" => Some(LeadType::RESP),
            _ => None,
        }
    }

    /// 获取导联名称
    pub fn name(&self) -> &'static str {
        match self {
            LeadType::I => "I",
            LeadType::II => "II",
            LeadType::III => "III",
            LeadType::AVR => "aVR",
            LeadType::AVL => "aVL",
            LeadType::AVF => "aVF",
            LeadType::V1 => "V1",
            LeadType::V2 => "V2",
            LeadType::V3 => "V3",
            LeadType::V4 => "V4",
            LeadType::V5 => "V5",
            LeadType::V6 => "V6",
            LeadType::IBP => "IBP",
            LeadType::SPO2 => "SpO2",
            LeadType::RESP => "Resp",
        }
    }
}

impl fmt::Display for LeadType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// 报警优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlarmPriority {
    /// 高优先级
    H,
    /// 中优先级
    PM,
    /// 低优先级
    PL,
}

impl AlarmPriority {
    /// 从字符串解析优先级
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "H" => Some(AlarmPriority::H),
            "PM" => Some(AlarmPriority::PM),
            "PL" => Some(AlarmPriority::PL),
            _ => None,
        }
    }
}

impl fmt::Display for AlarmPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlarmPriority::H => write!(f, "H"),
            AlarmPriority::PM => write!(f, "PM"),
            AlarmPriority::PL => write!(f, "PL"),
        }
    }
}

/// 生命体征数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VitalSignsData {
    /// 设备信息
    pub device: DeviceInfo,
    /// 采样时间
    pub sample_time: DateTime<Utc>,
    /// 心率（次/分）
    pub heart_rate: Option<i32>,
    /// 呼吸率（次/分）
    pub resp_rate: Option<i32>,
    /// 无创收缩压（mmHg）
    pub nibp_sys: Option<i32>,
    /// 无创舒张压（mmHg）
    pub nibp_dia: Option<i32>,
    /// 无创平均压（mmHg）
    pub nibp_map: Option<i32>,
    /// 有创收缩压（mmHg）
    pub ibp_sys: Option<i32>,
    /// 有创舒张压（mmHg）
    pub ibp_dia: Option<i32>,
    /// 有创平均压（mmHg）
    pub ibp_map: Option<i32>,
    /// 有创动脉率（次/分）
    pub ibp_rate: Option<i32>,
    /// 血氧饱和度（%）
    pub spo2: Option<i32>,
    /// 脉搏率（次/分）
    pub pulse_rate: Option<i32>,
    /// 灌注指数（%）
    pub pi: Option<f32>,
    /// 室早率（次/分）
    pub vpc_rate: Option<i32>,
}

/// 波形数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformData {
    /// 设备信息
    pub device: DeviceInfo,
    /// 导联类型
    pub lead: LeadType,
    /// 采样率（Hz）
    pub sample_rate: u16,
    /// 分辨率（mV/单位）
    pub resolution: f32,
    /// 波形起始时间
    pub start_time: DateTime<Utc>,
    /// 采样值数组（原始整数值）
    pub samples: Vec<i16>,
    /// 是否无效（导联脱落等）
    pub is_invalid: bool,
}

/// 报警事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEvent {
    /// 设备信息
    pub device: DeviceInfo,
    /// 报警代码
    pub alarm_code: String,
    /// 报警名称
    pub alarm_name: String,
    /// 报警优先级
    pub priority: AlarmPriority,
    /// 事件阶段（start/end）
    pub event_phase: String,
    /// 报警状态（active/inactive）
    pub alarm_state: String,
    /// 触发参数
    pub trigger_param: Option<String>,
    /// 触发值
    pub trigger_value: Option<f32>,
    /// 阈值范围
    pub threshold: Option<String>,
    /// 报警时间
    pub alarm_time: DateTime<Utc>,
}
