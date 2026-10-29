//! 使用真实日志文件的集成测试
//!
//! 从 log 文件夹中读取真实的 HL7 报文，
//! 验证解析器能否正确处理真实场景的数据。

use std::fs;
use std::path::Path;

use monitor_proto::Hl7Parser;
use monitor_proto::ParsedData;
use monitor_common::types::DeviceVendor;

/// 解析日志文件的辅助函数
fn parse_log_file(path: &str) -> Vec<(String, String)> {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("无法找到工作区根目录");
    let full_path = workspace_root.join(path);
    
    let content = fs::read_to_string(&full_path)
        .unwrap_or_else(|_| panic!("无法读取日志文件: {:?}", full_path));
    let mut messages = Vec::new();
    let mut current_ip = String::new();
    let mut current_msg = String::new();
    let mut in_message = false;

    for line in content.lines() {
        // 检测日志头：===== [时间] 来自 IP =====
        if line.starts_with("===== [") && line.contains("来自") {
            // 如果之前有消息，保存
            if in_message && !current_msg.is_empty() {
                let msg = current_msg.trim().to_string();
                if msg.contains("MSH|") && msg.contains("OBX|") {
                    messages.push((current_ip.clone(), msg));
                }
            }
            // 提取 IP - 使用更可靠的方法：找到 "来自 " 后面的 IP
            if let Some(from_pos) = line.find("来自 ") {
                let after_from = &line[from_pos + "来自 ".len()..];
                if let Some(end_pos) = after_from.find(" =====") {
                    current_ip = after_from[..end_pos].trim().to_string();
                }
            }
            current_msg = String::new();
            in_message = true;
        } else if in_message {
            // 检测帧尾 \x1C（文件分隔符）
            if line.contains('\x1C') {
                // 把 \x1C 之前的内容加上
                if let Some(pos) = line.find('\x1C') {
                    let before_1c = &line[..pos];
                    if !before_1c.is_empty() {
                        if !current_msg.is_empty() {
                            current_msg.push('\r');
                        }
                        current_msg.push_str(before_1c);
                    }
                }
                // 消息结束
                let msg = current_msg.trim().to_string();
                if msg.contains("MSH|") && msg.contains("OBX|") {
                    messages.push((current_ip.clone(), msg));
                }
                current_msg = String::new();
                in_message = false;
            } else if !line.is_empty() && !line.trim().is_empty() {
                // HL7 段用 \r 分隔，我们需要把每行用 \r 连起来
                if !current_msg.is_empty() {
                    current_msg.push('\r');
                }
                current_msg.push_str(line);
            }
        }
    }

    // 处理最后一条消息
    if in_message && !current_msg.is_empty() {
        let msg = current_msg.trim().to_string();
        if msg.contains("MSH|") && msg.contains("OBX|") {
            messages.push((current_ip, msg));
        }
    }

    messages
}

#[test]
fn test_parse_log_1_mindray() {
    let log_path = "log/1.log";
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("无法找到工作区根目录");
    let full_path = workspace_root.join(log_path);
    
    if !full_path.exists() {
        eprintln!("日志文件不存在，跳过测试: {:?}", full_path);
        return;
    }

    let messages = parse_log_file(log_path);
    assert!(!messages.is_empty(), "未解析到任何消息");

    let parser = Hl7Parser::new();
    let mut vital_count = 0;
    let mut waveform_count = 0;
    let mut alarm_count = 0;
    let mut error_count = 0;

    for (ip, msg) in &messages {
        match parser.parse(msg, ip) {
            Ok(ParsedData::VitalSigns(vital)) => {
                vital_count += 1;
                // 验证基本字段
                assert!(!vital.device.device_ip.is_empty());
                assert!(vital.sample_time.timestamp() > 0);
            }
            Ok(ParsedData::Waveform(waves)) => {
                waveform_count += 1;
                assert!(!waves.is_empty());
                for wave in &waves {
                    assert!(!wave.samples.is_empty());
                    assert!(wave.sample_rate > 0);
                }
            }
            Ok(ParsedData::Alarm(alarm)) => {
                alarm_count += 1;
                assert!(!alarm.alarm_code.is_empty());
            }
            Err(e) => {
                error_count += 1;
                eprintln!("解析失败 (IP: {}): {}", ip, e);
            }
        }
    }

    eprintln!(
        "日志 1.log 解析结果: 生命体征={}, 波形={}, 报警={}, 失败={}",
        vital_count, waveform_count, alarm_count, error_count
    );

    // 1.log 中应该主要是迈瑞的生命体征数据
    assert!(vital_count > 0, "应该解析到生命体征数据");
    // 错误率应该很低
    let total = vital_count + waveform_count + alarm_count;
    assert!(
        error_count < total / 10,
        "错误率过高: {}/{}",
        error_count,
        total
    );
}

#[test]
fn test_parse_log_2_mixed() {
    let log_path = "log/2.log";
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("无法找到工作区根目录");
    let full_path = workspace_root.join(log_path);
    
    if !full_path.exists() {
        eprintln!("日志文件不存在，跳过测试: {:?}", full_path);
        return;
    }

    let messages = parse_log_file(log_path);
    assert!(!messages.is_empty(), "未解析到任何消息");

    let parser = Hl7Parser::new();
    let mut vital_count = 0;
    let mut waveform_count = 0;
    let mut alarm_count = 0;
    let mut comen_count = 0;
    let mut mindray_count = 0;
    let mut error_count = 0;

    for (ip, msg) in &messages {
        match parser.parse(msg, ip) {
            Ok(ParsedData::VitalSigns(vital)) => {
                vital_count += 1;
                match vital.device.vendor {
                    DeviceVendor::Comen => comen_count += 1,
                    DeviceVendor::Mindray => mindray_count += 1,
                }
            }
            Ok(ParsedData::Waveform(waves)) => {
                waveform_count += 1;
                if let Some(wave) = waves.first() {
                    match wave.device.vendor {
                        DeviceVendor::Comen => comen_count += 1,
                        DeviceVendor::Mindray => mindray_count += 1,
                    }
                }
            }
            Ok(ParsedData::Alarm(alarm)) => {
                alarm_count += 1;
                match alarm.device.vendor {
                    DeviceVendor::Comen => comen_count += 1,
                    DeviceVendor::Mindray => mindray_count += 1,
                }
            }
            Err(e) => {
                error_count += 1;
                eprintln!("解析失败 (IP: {}): {}", ip, e);
            }
        }
    }

    eprintln!(
        "日志 2.log 解析结果: 生命体征={}, 波形={}, 报警={}, 科曼={}, 迈瑞={}, 失败={}",
        vital_count, waveform_count, alarm_count, comen_count, mindray_count, error_count
    );

    // 2.log 中应该包含科曼和迈瑞的混合数据
    assert!(vital_count > 0 || waveform_count > 0, "应该解析到数据");
    assert!(comen_count > 0, "应该有科曼设备的数据");
    assert!(mindray_count > 0, "应该有迈瑞设备的数据");
}
