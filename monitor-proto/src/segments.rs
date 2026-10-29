//! HL7 段解析模块
//!
//! 负责解析 HL7 消息的各个段（MSH、OBR、OBX 等）。

use chrono::{DateTime, FixedOffset, Utc};
use monitor_common::error::MonitorError;
use monitor_common::utils::{DEFAULT_COMP_SEP, DEFAULT_FIELD_SEP, DEFAULT_SUBCOMP_SEP};

/// HL7 消息类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    /// 生命体征 / 波形（ORU_R01）
    ORU_R01,
    /// 报警事件（ORU_R40）
    ORU_R40,
    /// 其他未知类型
    Other(String),
}

/// MSH 段（消息头）
#[derive(Debug, Clone)]
pub struct MshSegment {
    /// 字段分隔符
    pub field_separator: char,
    /// 编码字符
    pub encoding_characters: String,
    /// 发送方应用
    pub sending_application: String,
    /// 发送方机构
    pub sending_facility: String,
    /// 消息时间
    pub message_time: Option<DateTime<Utc>>,
    /// 消息类型
    pub message_type: MessageType,
    /// 消息控制 ID
    pub message_control_id: String,
    /// 处理 ID
    pub processing_id: String,
    /// 版本号
    pub version_id: String,
}

/// OBR 段（观察请求）
#[derive(Debug, Clone)]
pub struct ObrSegment {
    /// 设置 ID
    pub set_id: String,
    /// 观测仪编号
    pub placer_order_number: String,
    /// 检验仪编号
    pub filler_order_number: String,
    /// 观测标识符
    pub universal_service_id: String,
    /// 观测标识符编码
    pub universal_service_code: String,
    /// 观测标识符命名系统
    pub universal_service_name_system: String,
    /// 观测时间
    pub observation_time: Option<DateTime<Utc>>,
}

/// OBX 值类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObxValueType {
    /// 数值型
    NM,
    /// 字符串编码型（波形数据）
    NA,
    /// 编码值
    CWE,
    /// 字符串型
    ST,
    /// 其他类型
    Other(String),
}

/// OBX 段（观察结果）
#[derive(Debug, Clone)]
pub struct ObxSegment {
    /// 设置 ID（序号）
    pub set_id: String,
    /// 值类型
    pub value_type: ObxValueType,
    /// 观测标识符（MDC 代码）
    pub observation_id: String,
    /// 观测标识符编码
    pub observation_code: String,
    /// 观测标识符名称
    pub observation_name: String,
    /// 观测标识符命名系统
    pub observation_name_system: String,
    /// 子 ID
    pub sub_id: String,
    /// 观测值（原始字符串）
    pub observation_value: String,
    /// 单位
    pub units: String,
    /// 单位编码
    pub units_code: String,
    /// 单位命名系统
    pub units_name_system: String,
    /// 参考范围
    pub reference_range: String,
    /// 异常标志
    pub abnormal_flags: String,
    /// 结果状态
    pub result_status: String,
    /// 观测时间
    pub observation_time: Option<DateTime<Utc>>,
    /// 设备信息
    pub device_info: String,
}

/// 完整的 HL7 消息
#[derive(Debug, Clone)]
pub struct Hl7Message {
    /// MSH 段
    pub msh: MshSegment,
    /// OBR 段
    pub obr: ObrSegment,
    /// OBX 段列表
    pub obx_list: Vec<ObxSegment>,
}

/// 解析组件（^ 分隔）
fn parse_components(s: &str) -> Vec<&str> {
    s.split(DEFAULT_COMP_SEP).collect()
}

/// 解析子组件（& 分隔）
fn parse_subcomponents(s: &str) -> Vec<&str> {
    s.split(DEFAULT_SUBCOMP_SEP).collect()
}

/// 解析 HL7 时间字符串
///
/// 支持的格式：
/// - YYYYMMDDHHMMSS.ssss+ZZZZ
/// - YYYYMMDDHHMMSS+ZZZZ
/// - YYYYMMDDHHMM+ZZZZ
/// 等等
pub fn parse_hl7_datetime(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        return None;
    }

    // 先找时区偏移的位置
    let tz_start = s.find(|c: char| c == '+' || c == '-');

    let (datetime_part, tz_part) = match tz_start {
        Some(pos) => (&s[..pos], &s[pos..]),
        None => (s, "+0000"),
    };

    // 移除可能的小数部分（支持有小数点和无小数点两种情况）
    let datetime_clean = if let Some(dot_pos) = datetime_part.find('.') {
        &datetime_part[..dot_pos]
    } else if datetime_part.len() > 14 {
        // 没有小数点，但长度超过14位，说明后面的是毫秒或更小单位
        &datetime_part[..14]
    } else {
        datetime_part
    };

    // 根据长度确定格式
    let fmt = match datetime_clean.len() {
        14 => "%Y%m%d%H%M%S",
        12 => "%Y%m%d%H%M",
        10 => "%Y%m%d%H",
        8 => "%Y%m%d",
        _ => return None,
    };

    // 解析时区
    let offset = FixedOffset::from_str_timezone(tz_part)?;

    // 组合解析
    let full_str = format!("{}{}", datetime_clean, tz_part);
    let full_fmt = format!("{}%z", fmt);

    match DateTime::parse_from_str(&full_str, &full_fmt) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(_) => None,
    }
}

/// 解析 MSH 段
pub fn parse_msh(line: &str) -> Result<MshSegment, MonitorError> {
    let fields: Vec<&str> = line.split(DEFAULT_FIELD_SEP).collect();

    if fields.len() < 12 {
        return Err(MonitorError::ParseError(format!(
            "MSH 段字段不足，期望至少12个，实际{}个",
            fields.len()
        )));
    }

    // MSH-1: 字段分隔符（总是 |，但这里因为用 | 分割，所以 fields[0] 是 "MSH"，fields[1] 是 "^~\&"）
    let field_separator = DEFAULT_FIELD_SEP;

    // MSH-2: 编码字符
    let encoding_characters = fields[1].to_string();

    // MSH-3: 发送方应用
    let sending_application = fields[2].to_string();

    // MSH-4: 发送方机构
    let sending_facility = fields[3].to_string();

    // MSH-7: 消息时间
    let message_time = parse_hl7_datetime(fields[6]);

    // MSH-9: 消息类型
    let msg_type_components = parse_components(fields[8]);
    let message_type_str = if msg_type_components.len() >= 2 {
        format!("{}^{}", msg_type_components[0], msg_type_components[1])
    } else {
        fields[8].to_string()
    };

    let message_type = match message_type_str.as_str() {
        "ORU^R01" => MessageType::ORU_R01,
        "ORU^R40" => MessageType::ORU_R40,
        _ => MessageType::Other(message_type_str),
    };

    // MSH-10: 消息控制 ID
    let message_control_id = fields[9].to_string();

    // MSH-11: 处理 ID
    let processing_id = fields[10].to_string();

    // MSH-12: 版本号
    let version_id = fields[11].to_string();

    Ok(MshSegment {
        field_separator,
        encoding_characters,
        sending_application,
        sending_facility,
        message_time,
        message_type,
        message_control_id,
        processing_id,
        version_id,
    })
}

/// 解析 OBR 段
pub fn parse_obr(line: &str) -> Result<ObrSegment, MonitorError> {
    let fields: Vec<&str> = line.split(DEFAULT_FIELD_SEP).collect();

    if fields.len() < 8 {
        return Err(MonitorError::ParseError(format!(
            "OBR 段字段不足，期望至少8个，实际{}个",
            fields.len()
        )));
    }

    // OBR-1: Set ID
    let set_id = fields[1].to_string();

    // OBR-2: Placer Order Number
    let placer_order_number = fields[2].to_string();

    // OBR-3: Filler Order Number
    let filler_order_number = fields[3].to_string();

    // OBR-4: Universal Service ID
    let service_components = parse_components(fields[4]);
    let universal_service_code = if !service_components.is_empty() {
        service_components[0].to_string()
    } else {
        String::new()
    };
    let universal_service_id = if service_components.len() > 1 {
        service_components[1].to_string()
    } else {
        fields[4].to_string()
    };
    let universal_service_name_system = if service_components.len() > 2 {
        service_components[2].to_string()
    } else {
        String::new()
    };

    // OBR-7: 观测时间
    let observation_time = parse_hl7_datetime(fields[6]);

    Ok(ObrSegment {
        set_id,
        placer_order_number,
        filler_order_number,
        universal_service_id,
        universal_service_code,
        universal_service_name_system,
        observation_time,
    })
}

/// 解析 OBX 段
pub fn parse_obx(line: &str) -> Result<ObxSegment, MonitorError> {
    let fields: Vec<&str> = line.split(DEFAULT_FIELD_SEP).collect();

    // OBX 段最少需要 6 个字段（OBX-0 到 OBX-5）
    if fields.len() < 6 {
        return Err(MonitorError::ParseError(format!(
            "OBX 段字段不足，期望至少6个，实际{}个",
            fields.len()
        )));
    }

    // OBX-1: Set ID
    let set_id = fields[1].to_string();

    // OBX-2: Value Type
    let value_type = match fields[2] {
        "NM" => ObxValueType::NM,
        "NA" => ObxValueType::NA,
        "CWE" => ObxValueType::CWE,
        "ST" => ObxValueType::ST,
        other => ObxValueType::Other(other.to_string()),
    };

    // OBX-3: Observation Identifier
    let obs_components = parse_components(fields[3]);
    let observation_code = if !obs_components.is_empty() {
        obs_components[0].to_string()
    } else {
        String::new()
    };
    let observation_name = if obs_components.len() > 1 {
        obs_components[1].to_string()
    } else {
        String::new()
    };
    let observation_name_system = if obs_components.len() > 2 {
        obs_components[2].to_string()
    } else {
        String::new()
    };
    let observation_id = fields[3].to_string();

    // OBX-4: Sub ID
    let sub_id = fields[4].to_string();

    // OBX-5: Observation Value
    let observation_value = fields[5].to_string();

    // OBX-6: Units
    let (units_code, units, units_name_system) = if fields.len() > 6 {
        let unit_components = parse_components(fields[6]);
        let code = if !unit_components.is_empty() {
            unit_components[0].to_string()
        } else {
            String::new()
        };
        let u = if unit_components.len() > 1 {
            unit_components[1].to_string()
        } else {
            String::new()
        };
        let sys = if unit_components.len() > 2 {
            unit_components[2].to_string()
        } else {
            String::new()
        };
        (code, u, sys)
    } else {
        (String::new(), String::new(), String::new())
    };

    // OBX-7: Reference Range
    let reference_range = if fields.len() > 7 {
        fields[7].to_string()
    } else {
        String::new()
    };

    // OBX-8: Abnormal Flags
    let abnormal_flags = if fields.len() > 8 {
        fields[8].to_string()
    } else {
        String::new()
    };

    // OBX-11: Result Status
    let result_status = if fields.len() > 11 {
        fields[11].to_string()
    } else {
        String::new()
    };

    // OBX-14: Observation Date/Time
    let observation_time = if fields.len() > 14 {
        parse_hl7_datetime(fields[14])
    } else {
        None
    };

    // OBX-18: Device
    let device_info = if fields.len() > 18 {
        fields[18].to_string()
    } else {
        String::new()
    };

    Ok(ObxSegment {
        set_id,
        value_type,
        observation_id,
        observation_code,
        observation_name,
        observation_name_system,
        sub_id,
        observation_value,
        units,
        units_code,
        units_name_system,
        reference_range,
        abnormal_flags,
        result_status,
        observation_time,
        device_info,
    })
}

/// 解析完整的 HL7 消息（帧内容字符串）
pub fn parse_message(content: &str) -> Result<Hl7Message, MonitorError> {
    // HL7 段之间用 \r 分隔
    let lines: Vec<&str> = content.split('\r').collect();

    let mut msh: Option<MshSegment> = None;
    let mut obr: Option<ObrSegment> = None;
    let mut obx_list: Vec<ObxSegment> = Vec::new();

    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        if line.starts_with("MSH") {
            msh = Some(parse_msh(line)?);
        } else if line.starts_with("OBR") {
            obr = Some(parse_obr(line)?);
        } else if line.starts_with("OBX") {
            obx_list.push(parse_obx(line)?);
        }
        // 忽略其他段（PID、PV1 等）
    }

    let msh = msh.ok_or_else(|| MonitorError::ParseError("消息中缺少 MSH 段".to_string()))?;
    let obr = obr.ok_or_else(|| MonitorError::ParseError("消息中缺少 OBR 段".to_string()))?;

    Ok(Hl7Message {
        msh,
        obr,
        obx_list,
    })
}

// FixedOffset 的辅助 trait，用于解析时区字符串
trait TimezoneParse {
    fn from_str_timezone(s: &str) -> Option<FixedOffset>;
}

impl TimezoneParse for FixedOffset {
    fn from_str_timezone(s: &str) -> Option<FixedOffset> {
        // 格式: +0800 或 -0500
        if s.len() != 5 {
            return None;
        }
        let sign = match s.chars().next()? {
            '+' => 1,
            '-' => -1,
            _ => return None,
        };
        let hours: i32 = s[1..3].parse().ok()?;
        let minutes: i32 = s[3..5].parse().ok()?;
        let total_seconds = sign * (hours * 3600 + minutes * 60);
        FixedOffset::east_opt(total_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_msh_mindray() {
        let msh_line = "MSH|^~\\&|MINDRAY_N_SERIES^00A037009B020689^EUI-64|MINDRAY|||20260626104815.0000+0800||ORU^R01^ORU_R01|13055|P|2.6|||AL|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO";
        let msh = parse_msh(msh_line).unwrap();
        assert_eq!(msh.sending_facility, "MINDRAY");
        assert_eq!(msh.message_type, MessageType::ORU_R01);
        assert_eq!(msh.message_control_id, "13055");
        assert_eq!(msh.version_id, "2.6");
        assert!(msh.message_time.is_some());
    }

    #[test]
    fn test_parse_msh_comen() {
        let msh_line = "MSH|^~\\&|NDMonitor|KC753621663G|||20260626104652.5980+0000||ORU^R01^ORU_R01|725|P|2.6|||NE|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO";
        let msh = parse_msh(msh_line).unwrap();
        assert_eq!(msh.sending_application, "NDMonitor");
        assert_eq!(msh.sending_facility, "KC753621663G");
        assert_eq!(msh.message_type, MessageType::ORU_R01);
        assert!(msh.message_time.is_some());
    }

    #[test]
    fn test_parse_obx_nm() {
        let obx_line = "OBX|1|NM|151578^MDC_TTHOR_RESP_RATE^MDC|1.7.1.151578|12|264928^MDC_DIM_RESP_PER_MIN^MDC|||||R|||20260626104815.0000+0800||||00A037009B020689^^00A037009B020689^EUI-64";
        let obx = parse_obx(obx_line).unwrap();
        assert_eq!(obx.set_id, "1");
        assert_eq!(obx.value_type, ObxValueType::NM);
        assert_eq!(obx.observation_code, "151578");
        assert_eq!(obx.observation_name, "MDC_TTHOR_RESP_RATE");
        assert_eq!(obx.observation_value, "12");
        assert_eq!(obx.units_code, "264928");
        assert_eq!(obx.result_status, "R");
    }

    #[test]
    fn test_parse_hl7_datetime() {
        let dt = parse_hl7_datetime("20260626104815.0000+0800").unwrap();
        assert!(dt.year() == 2026);

        let dt2 = parse_hl7_datetime("20260626104652.5980+0000").unwrap();
        assert!(dt2.year() == 2026);
    }
}
