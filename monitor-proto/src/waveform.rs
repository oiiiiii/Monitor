//! 波形数据解析器
//!
//! 解析科曼和迈瑞设备的心电波形 OBX 段，
//! 输出标准化的波形数据结构。

use monitor_common::error::{MonitorError, MonitorResult};
use monitor_common::types::{DeviceInfo, DeviceVendor, LeadType, WaveformData};

use crate::segments::{Hl7Message, ObxSegment, ObxValueType};
use crate::mdc_codes;

/// 波形解析器
pub struct WaveformParser {
    // 预留配置空间
}

impl WaveformParser {
    /// 创建新的解析器
    pub fn new() -> Self {
        Self {}
    }

    /// 解析波形消息，返回所有导联的波形数据
    pub fn parse(
        &self,
        message: &Hl7Message,
        device: &DeviceInfo,
    ) -> MonitorResult<Vec<WaveformData>> {
        let start_time = message
            .obr
            .observation_time
            .or_else(|| message.msh.message_time)
            .ok_or_else(|| MonitorError::ParseError("无法获取波形起始时间".to_string()))?;

        // 按 sub_id 分组，同一 sub_id 下的 OBX 属于同一个导联
        let mut lead_groups: std::collections::HashMap<String, Vec<&ObxSegment>> =
            std::collections::HashMap::new();

        for obx in &message.obx_list {
            let sub_prefix = self.get_sub_id_prefix(&obx.sub_id);
            lead_groups
                .entry(sub_prefix)
                .or_default()
                .push(obx);
        }

        let mut waveforms = Vec::new();

        for (_sub_prefix, obx_list) in lead_groups.iter() {
            // 找到波形数据 OBX（NA 类型，且 observation_code 是导联代码）
            if let Some(wave_obx) = self.find_wave_data_obx(obx_list) {
                let lead = match LeadType::from_str(&wave_obx.observation_code) {
                    Some(l) => l,
                    None => {
                        // 尝试从 observation_name 识别
                        if let Some(l) = LeadType::from_str(&wave_obx.observation_name) {
                            l
                        } else {
                            tracing::warn!(
                                "无法识别的导联代码: {}",
                                wave_obx.observation_code
                            );
                            continue;
                        }
                    }
                };

                // 解析采样率
                let sample_rate = self.parse_sample_rate(obx_list);

                // 解析分辨率
                let resolution = self.parse_resolution(obx_list, &device.vendor);

                // 解析 INOP 标志
                let is_invalid = self.parse_inop(obx_list);

                // 解析采样数据
                let samples = self.parse_samples(&wave_obx.observation_value, &device.vendor)?;

                waveforms.push(WaveformData {
                    device: device.clone(),
                    lead,
                    sample_rate,
                    resolution,
                    start_time,
                    samples,
                    is_invalid,
                });
            }
        }

        Ok(waveforms)
    }

    /// 获取 sub_id 的导联前缀（导联分组用）
    fn get_sub_id_prefix(&self, sub_id: &str) -> String {
        let parts: Vec<&str> = sub_id.split('.').collect();
        if parts.len() >= 4 {
            parts[..4].join(".")
        } else {
            sub_id.to_string()
        }
    }

    /// 找到波形数据 OBX（NA 类型）
    fn find_wave_data_obx<'a>(&self, obx_list: &[&'a ObxSegment]) -> Option<&'a ObxSegment> {
        obx_list.iter().find(|obx| {
            matches!(obx.value_type, ObxValueType::NA)
                && mdc_codes::MDC_LEAD_CODES.contains_key(obx.observation_code.as_str())
        }).copied()
    }

    /// 解析采样率
    fn parse_sample_rate(&self, obx_list: &[&ObxSegment]) -> u16 {
        for obx in obx_list {
            if mdc_codes::is_sample_rate_obx(&obx.observation_code, &obx.observation_name_system)
                || obx.observation_name.to_uppercase().contains("SAMP_RATE")
                || obx.observation_name.to_uppercase().contains("SAMPLE_RATE")
            {
                if let Ok(rate) = obx.observation_value.parse::<u16>() {
                    return rate;
                }
            }
        }
        // 默认值
        250
    }

    /// 解析分辨率
    fn parse_resolution(&self, obx_list: &[&ObxSegment], vendor: &DeviceVendor) -> f32 {
        // 迈瑞：从 OBX 中解析
        if matches!(vendor, DeviceVendor::Mindray) {
            for obx in obx_list {
                if obx.observation_code == "2327"
                    || obx.observation_name == "MDC_ATTR_NU_MSMT_RES"
                {
                    if let Ok(res) = obx.observation_value.parse::<f32>() {
                        return res;
                    }
                }
            }
            // 迈瑞默认分辨率
            return 0.001221;
        }

        // 科曼：需要增益系数，这里先返回默认值 1.0，后续配置
        1.0
    }

    /// 解析 INOP（导联不可操作）标志
    fn parse_inop(&self, obx_list: &[&ObxSegment]) -> bool {
        for obx in obx_list {
            if mdc_codes::is_inop_obx(&obx.observation_code, &obx.observation_name) {
                // INOP 值为 32767 表示导联不可操作
                if let Ok(val) = obx.observation_value.parse::<i32>() {
                    if val == 32767 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 解析采样值数组
    fn parse_samples(&self, value_str: &str, vendor: &DeviceVendor) -> MonitorResult<Vec<i16>> {
        if value_str.is_empty() {
            return Ok(Vec::new());
        }

        let mut samples = Vec::new();
        for part in value_str.split('^') {
            if part.is_empty() {
                continue;
            }

            match vendor {
                DeviceVendor::Mindray => {
                    // 迈瑞：有符号整数
                    match part.parse::<i16>() {
                        Ok(val) => samples.push(val),
                        Err(_) => {
                            tracing::warn!("无法解析迈瑞波形采样值: {}", part);
                        }
                    }
                }
                DeviceVendor::Comen => {
                    // 科曼：无符号 12 位整数（0-4095），存储为 i16
                    match part.parse::<u16>() {
                        Ok(val) => {
                            // 科曼基线是 2046，我们存原始值，展示时再处理
                            samples.push(val as i16);
                        }
                        Err(_) => {
                            tracing::warn!("无法解析科曼波形采样值: {}", part);
                        }
                    }
                }
            }
        }

        Ok(samples)
    }
}

impl Default for WaveformParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segments::parse_message;

    #[test]
    fn test_parse_mindray_waveform() {
        let content = "MSH|^~\\&|MINDRAY_N_SERIES^00A037009B020689^EUI-64|MINDRAY|||20260626105849.0000+0800||ORU^R01^ORU_R01|1|P|2.6|||AL|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO\rPID|||^^^Hospital^PI||^^^^^^L|||||unknownrace\rPV1||I|^^2\rOBR|1||1^MINDRAY_N_SERIES^00A037009B020689^EUI-64|CONTINUOUS WAVEFORM|||20260626105849.0000+0800|20260626105850.0000+0800\rOBX|1|NA|131329^MDC_ECG_ELEC_POTL_I^MDC|1.7.6.131329|-64^-80^-92^-96^-96^-88^-68^-36^4^56|266418^MDC_DIM_MILLI_VOLT^MDC|||||R\rOBX|2|NM|0^MDC_ATTR_SAMP_RATE^MDC|1.7.6.131329.1|500|264640^MDC_DIM_HZ^MDC|||||R\rOBX|3|NM|2327^MDC_ATTR_NU_MSMT_RES^MDC|1.7.6.131329.2|0.001221|266418^MDC_DIM_MILLI_VOLT^MDC|||||R\rOBX|4|NM|262196^MDC_EVT_INOP^MDC|1.7.6.131329.3|32767||||||R";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.66".to_string(),
            vendor: DeviceVendor::Mindray,
            model: "N_SERIES".to_string(),
            serial: "00A037009B020689".to_string(),
            bed_no: 2,
        };

        let parser = WaveformParser::new();
        let waveforms = parser.parse(&message, &device).unwrap();

        assert!(!waveforms.is_empty());
        let wave = &waveforms[0];
        assert_eq!(wave.lead, LeadType::I);
        assert_eq!(wave.sample_rate, 500);
        assert_eq!(wave.resolution, 0.001221);
        assert_eq!(wave.samples.len(), 10);
        assert_eq!(wave.samples[0], -64);
        assert_eq!(wave.samples[1], -80);
        assert!(wave.is_invalid); // 32767 INOP
    }

    #[test]
    fn test_parse_comen_waveform() {
        let content = "MSH|^~\\&|NDMonitor|KC753621663G|||20260626104652.6390+0000||ORU^R01^ORU_R01|726|P|2.6|||NE|NE||UNICODE UTF-8|||IHE_PCD_001^IHE PCD^1.3.6.1.4.1.19376.1.6.1.1.1^ISO\rPID||||||||M||UnKnown\rPV1||I|^^7^&107|R\rOBR|||726^NDMonitor^KC753621663G^EUI-64|3^MON_WAVE^99COMEN|||20260626104652.6430+0000\rOBX||NA|131329^MDC_ECG_ELEC_POTL_I^MDC|1.7.6.131329|2046^2046^2046^2046^2046|262656^MDC_DIM_DIMLESS^MDC|||||F\rOBX||NM|0^MDC_ATTR_SAMP_RATE^MDC|1.7.6.131329.1|250|264608^MDC_DIM_PER_SEC^MDC|||||F";

        let message = parse_message(content).unwrap();
        let device = DeviceInfo {
            device_ip: "10.10.12.63".to_string(),
            vendor: DeviceVendor::Comen,
            model: "NDMonitor".to_string(),
            serial: "KC753621663G".to_string(),
            bed_no: 7,
        };

        let parser = WaveformParser::new();
        let waveforms = parser.parse(&message, &device).unwrap();

        assert!(!waveforms.is_empty());
        let wave = &waveforms[0];
        assert_eq!(wave.lead, LeadType::I);
        assert_eq!(wave.sample_rate, 250);
        assert_eq!(wave.samples.len(), 5);
        assert_eq!(wave.samples[0], 2046); // 科曼原始值，基线
        assert!(!wave.is_invalid);
    }
}
