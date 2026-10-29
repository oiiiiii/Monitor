//! 报警事件解析器
//!
//! 解析迈瑞设备的 ORU_R40 报警消息，
//! 输出标准化的报警事件数据结构。

use monitor_common::error::{MonitorError, MonitorResult};
use monitor_common::types::{AlarmEvent, AlarmPriority, DeviceInfo};

use crate::segments::Hl7Message;

/// 报警解析器
pub struct AlarmParser {
    // 预留配置空间
}

impl AlarmParser {
    /// 创建新的解析器
    pub fn new() -> Self {
        Self {}
    }

    /// 解析报警消息
    pub fn parse(&self, message: &Hl7Message, device: &DeviceInfo) -> MonitorResult<AlarmEvent> {
        let alarm_time = message
            .obr
            .observation_time
            .or_else(|| message.msh.message_time)
            .ok_or_else(|| MonitorError::ParseError("无法获取报警时间".to_string()))?;

        // 从 OBX 列表中提取报警信息
        let mut alarm_code = String::new();
        let mut alarm_name = String::new();
        let mut priority = AlarmPriority::PL;
        let mut event_phase = String::new();
        let mut alarm_state = String::new();
        let mut trigger_param = None;
        let mut trigger_value = None;
        let mut threshold = None;

        // 第一个 OBX 通常是报警主信息
        if let Some(first_obx) = message.obx_list.first() {
            alarm_code = first_obx.observation_code.clone();
            alarm_name = first_obx.observation_name.clone();

            // 从 OBX-8（异常标志）解析优先级
            // 格式类似: L~PM~SP 或 A~PL~ST
            let flags = &first_obx.abnormal_flags;
            if let Some(p) = self.parse_priority_from_flags(flags) {
                priority = p;
            }
        }

        // 遍历其他 OBX 提取属性
        for obx in message.obx_list[1..].iter() {
            let code = &obx.observation_code;
            let name = &obx.observation_name;
            let value = &obx.observation_value;

            match code.as_str() {
                // 事件阶段
                "68481" | "MDC_ATTR_EVENT_PHASE" => {
                    event_phase = value.clone();
                }
                // 报警状态
                "68482" | "MDC_ATTR_ALARM_STATE" => {
                    alarm_state = value.clone();
                }
                // 报警优先级
                "68484" | "MDC_ATTR_ALARM_PRIORITY" => {
                    if let Some(p) = AlarmPriority::from_str(value) {
                        priority = p;
                    }
                }
                // 报警类型
                "68485" | "MDC_ATTR_ALERT_TYPE" => {
                    // SP=参数报警, ST=状态报警
                }
                // 触发参数（触发参数是数值型，且代码是已知的生命体征参数
                _ => {
                    // 如果是数值型，可能是触发参数
                    if !value.is_empty() && value.parse::<f32>().is_ok() {
                        if trigger_param.is_none() {
                            trigger_param = Some(name.clone());
                            trigger_value = value.parse::<f32>().ok();
                            threshold = if !obx.reference_range.is_empty() {
                                Some(obx.reference_range.clone())
                            } else {
                                None
                            };
                        }
                    }
                }
            }
        }

        Ok(AlarmEvent {
            device: device.clone(),
            alarm_code,
            alarm_name,
            priority,
            event_phase,
            alarm_state,
            trigger_param,
            trigger_value,
            threshold,
            alarm_time,
        })
    }

    /// 从异常标志中解析报警优先级
    ///
    /// 格式: 类似 "L~PM~SP" 或 "A~PL~ST"
    /// 第二个波浪号分隔，第二段是优先级
    fn parse_priority_from_flags(&self, flags: &str) -> Option<AlarmPriority> {
        let parts: Vec<&str> = flags.split('~').collect();
        if parts.len() >= 2 {
            AlarmPriority::from_str(parts[1])
        } else {
            None
        }
    }
}

impl Default for AlarmParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segments::parse_message;
    use monitor_common::types::DeviceVendor;

    #[test]
    fn test_parse_alarm_low_pressure() {
        let content = "MSH|^~\\&|MINDRAY_N_SERIES^00A037009B020689^EUI-64|MINDRAY|||20260626110038+0800||ORU^R40^ORU_R40|1|P|2.6|||AL|NE||UNICODE UTF-8|||IHE_PCD_ACM_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.4.1^ISO\rPID|||^^^Hospital^PI||^^^^^^L|||||unknownrace\rPV1||I|^^2\rOBR|1||1^MINDRAY_N_SERIES^00A037009B020689^EUI-64|196616^MDC_EVT_ALARM^MDC|||20260626110038+0800\rOBX|1|CWE|196616^MDC_EVT_ALARM^MDC|1.1.1.150039.1|196674^MDC_EVT_LO_VAL_LT_LIM^MDC|||L~PM~SP|||R|||20260626110038+0800\rOBX|2|NM|150039^MDC_PRESS_BLD_ART_ABP_MEAN^MDC|1.1.1.150039.2|61|266016^MDC_DIM_MMHG^MDC|70-110||||R\rOBX|3|ST|68481^MDC_ATTR_EVENT_PHASE^MDC|1.1.1.150039.3|start||||||R\rOBX|4|ST|68482^MDC_ATTR_ALARM_STATE^MDC|1.1.1.150039.4|active||||||R\rOBX|5|ST|68483^MDC_ATTR_ALARM_INACTIVATION_STATE^MDC|1.1.1.150039.5|||||||R\rOBX|6|ST|68484^MDC_ATTR_ALARM_PRIORITY^MDC|1.1.1.150039.6|PM||||||R\rOBX|7|ST|68485^MDC_ATTR_ALERT_TYPE^MDC|1.1.1.150039.7|SP||||||R";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.65".to_string(),
            vendor: DeviceVendor::Mindray,
            model: "N_SERIES".to_string(),
            serial: "00A037009B020689".to_string(),
            bed_no: 1,
        };

        let parser = AlarmParser::new();
        let alarm = parser.parse(&message, &device).unwrap();

        assert_eq!(alarm.alarm_code, "196616");
        assert_eq!(alarm.alarm_name, "MDC_EVT_ALARM");
        assert_eq!(alarm.priority, AlarmPriority::PM);
        assert_eq!(alarm.event_phase, "start");
        assert_eq!(alarm.alarm_state, "active");
        assert_eq!(alarm.trigger_value, Some(61.0));
        assert_eq!(alarm.threshold, Some("70-110".to_string()));
    }

    #[test]
    fn test_parse_alarm_lead_off() {
        let content = "MSH|^~\\&|MINDRAY_N_SERIES^00A037009B020689^EUI-64|MINDRAY|||20260626105931.0000+0800||ORU^R40^ORU_R40|1|P|2.6|||AL|NE||UNICODE UTF-8|||IHE_PCD_ACM_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.4.1^ISO\rPID|||^^^Hospital^PI||^^^^^^L|||||unknownrace\rPV1||I|^^2\rOBR|1||1^MINDRAY_N_SERIES^00A037009B020689^EUI-64|196616^MDC_EVT_ALARM^MDC|||20260626105931.0000+0800\rOBX|1|CWE|196616^MDC_EVT_ALARM^MDC|1.7.0.196680.1|196680^MDC_EVT_LEAD_OFF^MDC|||A~PL~ST|||R|||20260626105931.0000+0800||||00A037009B020689^^00A037009B020689^EUI-64||460416^MDC_TRUNK_BREAST^MDC\rOBX|2|CWE|68480^MDC_ATTR_ALERT_SOURCE^MDC|1.7.0.196680.2|69953^MDC_DEV_MON_PT_PHYSIO_MULTI_PARAM_MDS^MDC||||||R\rOBX|3|ST|68481^MDC_ATTR_EVENT_PHASE^MDC|1.7.0.196680.3|start||||||R\rOBX|4|ST|68482^MDC_ATTR_ALARM_STATE^MDC|1.7.0.196680.4|active||||||R\rOBX|5|ST|68483^MDC_ATTR_ALARM_INACTIVATION_STATE^MDC|1.7.0.196680.5|||||||R\rOBX|6|ST|68484^MDC_ATTR_ALARM_PRIORITY^MDC|1.7.0.196680.6|PL||||||R\rOBX|7|ST|68485^MDC_ATTR_ALERT_TYPE^MDC|1.7.0.196680.7|ST||||||R";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.66".to_string(),
            vendor: DeviceVendor::Mindray,
            model: "N_SERIES".to_string(),
            serial: "00A037009B020689".to_string(),
            bed_no: 2,
        };

        let parser = AlarmParser::new();
        let alarm = parser.parse(&message, &device).unwrap();

        assert_eq!(alarm.alarm_code, "196616");
        assert_eq!(alarm.priority, AlarmPriority::PL);
        assert_eq!(alarm.event_phase, "start");
        assert_eq!(alarm.alarm_state, "active");
    }
}
