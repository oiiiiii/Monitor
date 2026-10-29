//! HL7 解析器主入口
//!
//! 提供高层解析接口，自动识别设备类型并路由到对应解析器。

use monitor_common::error::MonitorResult;
use monitor_common::types::{
    AlarmEvent, DeviceInfo, DeviceVendor, VitalSignsData, WaveformData,
};

use crate::segments::{parse_message, Hl7Message, MessageType, ObxValueType};
use crate::mdc_codes::vendor;

/// HL7 解析器
pub struct Hl7Parser {
    vital_parser: crate::vital::VitalSignsParser,
    waveform_parser: crate::waveform::WaveformParser,
    alarm_parser: crate::alarm::AlarmParser,
}

impl Hl7Parser {
    /// 创建新的解析器
    pub fn new() -> Self {
        Self {
            vital_parser: crate::vital::VitalSignsParser::new(),
            waveform_parser: crate::waveform::WaveformParser::new(),
            alarm_parser: crate::alarm::AlarmParser::new(),
        }
    }

    /// 解析 HL7 消息（帧内容字符串）
    ///
    /// 返回解析结果枚举：生命体征、波形、或报警
    pub fn parse(&self, content: &str, device_ip: &str) -> MonitorResult<ParsedData> {
        let message = parse_message(content)?;
        let device_info = self.extract_device_info(&message, device_ip)?;

        match message.msh.message_type {
            MessageType::ORU_R40 => {
                // 报警消息
                let alarm = self.alarm_parser.parse(&message, &device_info)?;
                Ok(ParsedData::Alarm(alarm))
            }
            MessageType::ORU_R01 => {
                // 需要根据 OBR 判断是体征还是波形
                if self.is_waveform_message(&message) {
                    let waveforms = self.waveform_parser.parse(&message, &device_info)?;
                    Ok(ParsedData::Waveform(waveforms))
                } else {
                    let vital = self.vital_parser.parse(&message, &device_info)?;
                    Ok(ParsedData::VitalSigns(vital))
                }
            }
            MessageType::Other(_) => {
                // 未知类型，尝试按体征解析
                let vital = self.vital_parser.parse(&message, &device_info)?;
                Ok(ParsedData::VitalSigns(vital))
            }
        }
    }

    /// 从消息中提取设备信息
    fn extract_device_info(
        &self,
        message: &Hl7Message,
        device_ip: &str,
    ) -> MonitorResult<DeviceInfo> {
        let sending_app = &message.msh.sending_application;
        let sending_facility = &message.msh.sending_facility;

        // 判断厂商
        let vendor = if vendor::COMEN_KEYWORDS
            .iter()
            .any(|k| sending_app.contains(k) || sending_facility.contains(k))
        {
            DeviceVendor::Comen
        } else if vendor::MINDRAY_KEYWORDS
            .iter()
            .any(|k| sending_app.contains(k) || sending_facility.contains(k))
        {
            DeviceVendor::Mindray
        } else {
            // 默认按迈瑞处理，记录日志
            tracing::warn!(
                "无法识别设备厂商，sending_app={}, sending_facility={}, ip={}",
                sending_app,
                sending_facility,
                device_ip
            );
            DeviceVendor::Mindray
        };

        // 提取序列号
        let serial = self.extract_serial(sending_app, sending_facility);

        // 提取型号
        let model = match vendor {
            DeviceVendor::Comen => "NDMonitor".to_string(),
            DeviceVendor::Mindray => "N_SERIES".to_string(),
        };

        Ok(DeviceInfo {
            device_ip: device_ip.to_string(),
            vendor,
            model,
            serial,
            bed_no: 0, // 床位号需要外部配置映射
        })
    }

    /// 从 MSH 段提取设备序列号
    fn extract_serial(&self, sending_app: &str, sending_facility: &str) -> String {
        // 迈瑞格式: MINDRAY_N_SERIES^00A037009B020689^EUI-64
        if sending_app.contains('^') {
            let parts: Vec<&str> = sending_app.split('^').collect();
            if parts.len() >= 2 {
                return parts[1].to_string();
            }
        }
        // 科曼: 序列号在 sending_facility
        if !sending_facility.is_empty() && sending_facility != "MINDRAY" {
            return sending_facility.to_string();
        }
        String::new()
    }

    /// 判断是否为波形消息
    fn is_waveform_message(&self, message: &Hl7Message) -> bool {
        let service_id = &message.obr.universal_service_id;
        let service_code = &message.obr.universal_service_code;
        let name_system = &message.obr.universal_service_name_system;

        // 科曼波形: MON_WAVE / 99COMEN
        if service_id.contains("MON_WAVE") || name_system == "99COMEN" {
            return true;
        }

        // 迈瑞波形: CONTINUOUS WAVEFORM / 99MNDRY
        if service_id.contains("CONTINUOUS WAVEFORM")
            || service_id.contains("WAVEFORM")
            || name_system == "99MNDRY"
        {
            // 需要进一步确认：体征消息也可能是 99MNDRY 命名空间
            // 所以还要看 OBX 中是否有 NA 类型（波形数据类型）
            return message
                .obx_list
                .iter()
                .any(|obx| matches!(obx.value_type, ObxValueType::NA));
        }

        false
    }
}

impl Default for Hl7Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析结果枚举
#[derive(Debug, Clone)]
pub enum ParsedData {
    /// 生命体征数据
    VitalSigns(VitalSignsData),
    /// 波形数据（一帧可能包含多个导联）
    Waveform(Vec<WaveformData>),
    /// 报警事件
    Alarm(AlarmEvent),
}
