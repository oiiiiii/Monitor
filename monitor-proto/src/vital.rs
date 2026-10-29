//! 生命体征数据解析器
//!
//! 解析科曼和迈瑞设备的生命体征 OBX 段，
//! 输出标准化的生命体征数据结构。

use monitor_common::error::{MonitorError, MonitorResult};
use monitor_common::types::{DeviceInfo, VitalSignsData};

use crate::segments::Hl7Message;
use crate::mdc_codes;

/// 生命体征解析器
pub struct VitalSignsParser {
    // 预留配置空间
}

impl VitalSignsParser {
    /// 创建新的解析器
    pub fn new() -> Self {
        Self {}
    }

    /// 解析生命体征消息
    pub fn parse(&self, message: &Hl7Message, device: &DeviceInfo) -> MonitorResult<VitalSignsData> {
        let sample_time = message
            .obr
            .observation_time
            .or_else(|| message.msh.message_time)
            .ok_or_else(|| {
                MonitorError::ParseError("无法获取生命体征采样时间".to_string())
            })?;

        let mut vital = VitalSignsData {
            device: device.clone(),
            sample_time,
            heart_rate: None,
            resp_rate: None,
            nibp_sys: None,
            nibp_dia: None,
            nibp_map: None,
            ibp_sys: None,
            ibp_dia: None,
            ibp_map: None,
            ibp_rate: None,
            spo2: None,
            pulse_rate: None,
            pi: None,
            vpc_rate: None,
        };

        for obx in &message.obx_list {
            let code = &obx.observation_code;
            let name_system = &obx.observation_name_system;
            let value = &obx.observation_value;

            // 跳过空值
            if value.is_empty() {
                continue;
            }

            // 处理虚心数值（低置信度值，格式如 "(75)"）
            // 移除括号、空格等特殊字符
            let cleaned_value = value
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim();

            // 记录虚心数值处理日志
            if cleaned_value != value.trim() {
                tracing::debug!("虚心数值处理: 原始值='{}', 清理后='{}'", value, cleaned_value);
            }

            // 解析数值
            let num_value: Option<i32> = cleaned_value.parse().ok();
            let float_value: Option<f32> = cleaned_value.parse().ok();

            match name_system.as_str() {
                "MDC" => {
                    // 标准 MDC 编码
                    self.parse_mdc_vital(code, num_value, float_value, &mut vital);
                }
                "99MNDRY" => {
                    // 迈瑞私有编码
                    self.parse_mndry_vital(code, num_value, &mut vital);
                }
                _ => {
                    // 未识别的命名空间，尝试按 MDC 处理
                    self.parse_mdc_vital(code, num_value, float_value, &mut vital);
                }
            }
        }

        Ok(vital)
    }

    /// 解析标准 MDC 编码的生命体征
    fn parse_mdc_vital(
        &self,
        code: &str,
        int_val: Option<i32>,
        float_val: Option<f32>,
        vital: &mut VitalSignsData,
    ) {
        match code {
            // 心率
            "147842" => vital.heart_rate = int_val,

            // 呼吸率（多种编码兼容）
            "151578" | "151562" => vital.resp_rate = int_val,

            // 血氧饱和度
            "150456" => vital.spo2 = int_val,

            // 脉搏率
            "149530" | "149546" => {
                vital.pulse_rate = int_val;
            }

            // 灌注指数（多种编码兼容）
            "150488" | "160324" => vital.pi = float_val,

            // 无创血压 - 科曼使用标准MDC编码
            "150021" | "150301" => vital.nibp_sys = int_val,
            "150022" | "150302" => vital.nibp_dia = int_val,
            "150023" | "150303" => vital.nibp_map = int_val,

            // 有创血压 - 科曼和迈瑞使用不同编码
            "150037" | "150033" => vital.ibp_sys = int_val,
            "150038" | "150034" => vital.ibp_dia = int_val,
            "150039" | "150035" => vital.ibp_map = int_val,

            // 室早率
            "148066" => vital.vpc_rate = int_val,

            _ => {
                // 未识别的 MDC 代码，忽略
                tracing::debug!("未识别的 MDC 生命体征代码: {}", code);
            }
        }
    }

    /// 解析迈瑞私有编码的生命体征
    fn parse_mndry_vital(&self, code: &str, int_val: Option<i32>, vital: &mut VitalSignsData) {
        match code {
            // 血压脉搏率
            "364" => {
                vital.ibp_rate = int_val;
            }

            // 无创血压（迈瑞私有编码）
            "170" | "161" => {
                vital.nibp_sys = int_val;
            }
            "171" | "162" => {
                vital.nibp_dia = int_val;
            }
            "172" | "163" => {
                vital.nibp_map = int_val;
            }

            // 室早率相关（迈瑞私有编码）
            // 这里我们只取标准的室早率即可，私有编码用于扩展
            "108" | "352" | "298" | "299" | "302" => {
                // 这些都是心律失常参数，暂时忽略详细分类
                tracing::debug!("迈瑞私有心律失常参数: code={}", code);
            }

            _ => {
                tracing::debug!("未识别的迈瑞私有生命体征代码: {}", code);
            }
        }
    }
}

impl Default for VitalSignsParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segments::parse_message;

    #[test]
    fn test_parse_mindray_vital() {
        let content = "MSH|^~\\&|MINDRAY_N_SERIES^00A037009B077696^EUI-64|MINDRAY|||20260626104837000+0800||ORU^R01^ORU_R01|2224|P|2.6|||AL|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO\rPID|||^^^Hospital^PI||^^^^^^L|||||unknownrace\rPV1||I|^^1\rOBR|1|2224^MINDRAY_N_SERIES^00A037009B077696^EUI-64|2224^MINDRAY_N_SERIES^00A037009B077696^EUI-64|182777000^monitoring of patient^SCT|||20260626104837+0800\rOBX|1|NM|150037^MDC_PRESS_BLD_ART_ABP_SYS^MDC|1.1.1.150037|95|266016^MDC_DIM_MMHG^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|2|NM|150039^MDC_PRESS_BLD_ART_ABP_MEAN^MDC|1.1.1.150039|56|266016^MDC_DIM_MMHG^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|3|NM|150038^MDC_PRESS_BLD_ART_ABP_DIA^MDC|1.1.1.150038|37|266016^MDC_DIM_MMHG^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|4|NM|364^MNDRY_BLD_PULS_RATE_ART_ABP^99MNDRY|1.1.1.364|78|264864^MDC_DIM_BEAT_PER_MIN^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|5|NM|150456^MDC_PULS_OXIM_SAT_O2^MDC|1.3.1.150456|100|262688^MDC_DIM_PERCENT^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|6|NM|149530^MDC_PULS_OXIM_PULS_RATE^MDC|1.3.1.149530|78|264864^MDC_DIM_BEAT_PER_MIN^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|7|NM|150488^MDC_BLD_PERF_INDEX^MDC|1.3.1.150488|2.46|262688^MDC_DIM_PERCENT^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|8|NM|151578^MDC_TTHOR_RESP_RATE^MDC|1.7.1.151578|16|264928^MDC_DIM_RESP_PER_MIN^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|15|NM|147842^MDC_ECG_HEART_RATE^MDC|1.7.4.147842|78|264864^MDC_DIM_BEAT_PER_MIN^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.65".to_string(),
            vendor: monitor_common::types::DeviceVendor::Mindray,
            model: "N_SERIES".to_string(),
            serial: "00A037009B077696".to_string(),
            bed_no: 1,
        };

        let parser = VitalSignsParser::new();
        let vital = parser.parse(&message, &device).unwrap();

        assert_eq!(vital.heart_rate, Some(78));
        assert_eq!(vital.resp_rate, Some(16));
        assert_eq!(vital.ibp_sys, Some(95));
        assert_eq!(vital.ibp_dia, Some(37));
        assert_eq!(vital.ibp_map, Some(56));
        assert_eq!(vital.spo2, Some(100));
        assert_eq!(vital.pulse_rate, Some(78));
        assert_eq!(vital.pi, Some(2.46));
    }

    #[test]
    fn test_parse_comen_vital() {
        let content = "MSH|^~\\&|NDMonitor|KC753621663G|||20260626104652.5980+0000||ORU^R01^ORU_R01|725|P|2.6|||NE|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO\rPID||||||||M||UnKnown\rPV1||I|^^7^&107|R\rOBR||725^NDMonitor^KC753621663G^EUI-64|725^NDMonitor^KC753621663G^EUI-64|182777000^monitoring of patient^SCT|||20260626104652.5990+0000\rOBX||NM|149546^MDC_PULS_RATE_NON_INV^MDC|1.1.9.149546|91|264864^MDC_DIM_BEAT_PER_MIN^MDC|45^140||||F|||20260626104652.6210+0000||||KC753621663G^NDMonitor^KC753621663G^EUI-64";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.63".to_string(),
            vendor: monitor_common::types::DeviceVendor::Comen,
            model: "NDMonitor".to_string(),
            serial: "KC753621663G".to_string(),
            bed_no: 7,
        };

        let parser = VitalSignsParser::new();
        let vital = parser.parse(&message, &device).unwrap();

        assert_eq!(vital.pulse_rate, Some(91));
    }

    #[test]
    fn test_parse_parenthesized_value() {
        let content = "MSH|^~\\&|MINDRAY_N_SERIES^00A037009B077696^EUI-64|MINDRAY|||20260626104837000+0800||ORU^R01^ORU_R01|2224|P|2.6|||AL|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO\rPID|||^^^Hospital^PI||^^^^^^L|||||unknownrace\rPV1||I|^^1\rOBR|1|2224^MINDRAY_N_SERIES^00A037009B077696^EUI-64|2224^MINDRAY_N_SERIES^00A037009B077696^EUI-64|182777000^monitoring of patient^SCT|||20260626104837+0800\rOBX|1|NM|147842^MDC_ECG_HEART_RATE^MDC|1.7.4.147842|(75)|264864^MDC_DIM_BEAT_PER_MIN^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64\rOBX|2|NM|150488^MDC_BLD_PERF_INDEX^MDC|1.3.1.150488|(2.46)|262688^MDC_DIM_PERCENT^MDC|||||R|||20260626104837+0800||||00A037009B077696^^00A037009B077696^EUI-64";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.65".to_string(),
            vendor: monitor_common::types::DeviceVendor::Mindray,
            model: "N_SERIES".to_string(),
            serial: "00A037009B077696".to_string(),
            bed_no: 1,
        };

        let parser = VitalSignsParser::new();
        let vital = parser.parse(&message, &device).unwrap();

        assert_eq!(vital.heart_rate, Some(75));
        assert_eq!(vital.pi, Some(2.46));
    }
}
