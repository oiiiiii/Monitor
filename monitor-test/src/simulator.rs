//! HL7 设备模拟器
//!
//! 模拟监护仪发送 HL7 报文，用于全链路集成测试：
//! - 模拟 1-8 号床设备
//! - 持续发送波形 + 体征 + 报警
//! - 可配置发送速率和数据量
//!
//! 用法：
//!   cargo run --bin simulator -- --beds 4 --hr 75 --duration 60

use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

/// HL7 设备模拟器参数
#[derive(Parser, Debug)]
#[command(name = "simulator", about = "HL7 监护仪模拟器 - 用于集成测试")]
struct Args {
    /// 目标服务器地址
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// 目标端口
    #[arg(long, default_value_t = 9100)]
    port: u16,

    /// 模拟的床位数量（1-8）
    #[arg(long, default_value_t = 4)]
    beds: u16,

    /// 心率基准值
    #[arg(long, default_value_t = 75)]
    hr: i32,

    /// 波形发送间隔（毫秒）
    #[arg(long, default_value_t = 1000)]
    wave_interval_ms: u64,

    /// 体征发送间隔（秒）
    #[arg(long, default_value_t = 5)]
    vital_interval_secs: u64,

    /// 运行时长（秒，0 表示无限）
    #[arg(long, default_value_t = 0)]
    duration: u64,

    /// 设备厂商 (mindray/comen)
    #[arg(long, default_value = "mindray")]
    vendor: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,simulator=debug"));
    fmt().with_env_filter(filter).init();

    let args = Args::parse();

    info!("========================================");
    info!("  HL7 监护仪模拟器 - 集成测试工具");
    info!("========================================");
    info!("目标服务器: {}:{}", args.host, args.port);
    info!("模拟床位: {}", args.beds);
    info!("心率基准: {} bpm", args.hr);
    info!("波形间隔: {}ms", args.wave_interval_ms);
    info!("体征间隔: {}s", args.vital_interval_secs);
    if args.duration > 0 {
        info!("运行时长: {}s", args.duration);
    } else {
        info!("运行时长: 无限 (按 Ctrl+C 停止)");
    }

    // 启动每个床位的模拟任务
    let mut handles = Vec::new();

    for bed_no in 1..=args.beds {
        let host = args.host.clone();
        let port = args.port;
        let hr_base = args.hr;
        let wave_interval = Duration::from_millis(args.wave_interval_ms);
        let vital_interval = Duration::from_secs(args.vital_interval_secs);
        let duration = if args.duration > 0 {
            Some(Duration::from_secs(args.duration))
        } else {
            None
        };
        let vendor = args.vendor.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = simulate_bed(
                bed_no, &host, port, hr_base, wave_interval, vital_interval, duration, &vendor,
            )
            .await
            {
                error!(bed = bed_no, error = %e, "模拟任务失败");
            }
        });
        handles.push(handle);
    }

    // 等待 Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("收到停止信号，正在关闭模拟器...");

    info!("模拟器已停止");
    Ok(())
}

/// 模拟一个床位的设备
async fn simulate_bed(
    bed_no: u16,
    host: &str,
    port: u16,
    hr_base: i32,
    wave_interval: Duration,
    vital_interval: Duration,
    duration: Option<Duration>,
    vendor: &str,
) -> Result<()> {
    let addr = format!("{}:{}", host, port);

    info!(
        bed = bed_no,
        "正在连接到 {} ...", addr
    );

    // 连接 TCP
    let mut stream = TcpStream::connect(&addr).await?;
    info!(bed = bed_no, "已连接");

    let mut wave_count: u64 = 0;
    let mut vital_count: u64 = 0;

    let start = std::time::Instant::now();
    let mut last_vital = std::time::Instant::now();

    let mut rng = StdRng::from_entropy();

    loop {
        // 检查是否超时
        if let Some(dur) = duration {
            if start.elapsed() >= dur {
                break;
            }
        }

        // 生成并发送波形数据
        wave_count += 1;
        let wave_samples = generate_ecg_wave(hr_base, wave_count, 50);
        let wave_hl7 = build_wave_hl7(bed_no, vendor, &wave_samples);
        let frame_data = wrap_hl7_frame(&wave_hl7);

        if let Err(e) = stream.write_all(&frame_data).await {
            warn!(bed = bed_no, error = %e, "发送波形数据失败");
            break;
        }

        // 定时发送体征数据
        if last_vital.elapsed() >= vital_interval {
            last_vital = std::time::Instant::now();
            vital_count += 1;

            let hr = hr_base + rng.gen_range(-5..5);
            let spo2 = 98 + rng.gen_range(-2..3);
            let resp = 16 + rng.gen_range(-2..3);
            let sys = 120 + rng.gen_range(-5..5);
            let dia = 80 + rng.gen_range(-3..3);

            let vital_hl7 = build_vital_hl7(bed_no, vendor, hr, spo2, resp, sys, dia);
            let vital_data = wrap_hl7_frame(&vital_hl7);

            if let Err(e) = stream.write_all(&vital_data).await {
                warn!(bed = bed_no, error = %e, "发送体征数据失败");
                break;
            }

            // 每 5 次体征触发一次报警周期（开始 + 结束）
            if vital_count % 10 == 5 {
                let alarm_hl7 = build_alarm_hl7(
                    bed_no, vendor,
                    "MDC_ECG_TACHY", "心动过速",
                    "H", "start", hr, 100,
                );
                let alarm_data = wrap_hl7_frame(&alarm_hl7);
                if let Err(e) = stream.write_all(&alarm_data).await {
                    warn!(bed = bed_no, error = %e, "发送报警开始失败");
                    break;
                }
                info!(bed = bed_no, "发送报警: 心动过速 [开始]");
            }
            if vital_count % 10 == 8 {
                let alarm_hl7 = build_alarm_hl7(
                    bed_no, vendor,
                    "MDC_ECG_TACHY", "心动过速",
                    "H", "end", hr, 100,
                );
                let alarm_data = wrap_hl7_frame(&alarm_hl7);
                if let Err(e) = stream.write_all(&alarm_data).await {
                    warn!(bed = bed_no, error = %e, "发送报警结束失败");
                    break;
                }
                info!(bed = bed_no, "发送报警: 心动过速 [结束]");
            }

            info!(
                bed = bed_no,
                hr, spo2, resp, sys, dia,
                "体征 #{} (波形累计: {})",
                vital_count, wave_count
            );
        }

        // 等待下一个波形周期
        tokio::time::sleep(wave_interval).await;
    }

    info!(
        bed = bed_no,
        "模拟结束: 波形 {}, 体征 {}",
        wave_count, vital_count
    );

    Ok(())
}

/// 生成模拟心电波形（简化版）
fn generate_ecg_wave(hr: i32, sequence: u64, num_samples: usize) -> Vec<i16> {
    let mut samples = Vec::with_capacity(num_samples);
    let base_freq = hr as f64 / 60.0;
    let sample_rate = 500.0;

    let mut local_rng = StdRng::from_entropy();

    for i in 0..num_samples {
        let t = (sequence * num_samples as u64 + i as u64) as f64 / sample_rate;
        let phase = (t * base_freq) % 1.0;

        let value = if phase < 0.1 {
            50.0 * ((phase / 0.1) * std::f64::consts::PI).sin()
        } else if phase < 0.15 {
            0.0
        } else if phase < 0.2 {
            -100.0 * ((phase - 0.15) / 0.05 * std::f64::consts::PI).sin()
        } else if phase < 0.25 {
            800.0 * ((phase - 0.2) / 0.05 * std::f64::consts::PI).sin()
        } else if phase < 0.3 {
            -150.0 * ((phase - 0.25) / 0.05 * std::f64::consts::PI).sin()
        } else if phase < 0.4 {
            0.0
        } else if phase < 0.6 {
            150.0 * ((phase - 0.4) / 0.2 * std::f64::consts::PI).sin()
        } else {
            0.0
        };

        let baseline = 20.0 * (t * 0.1 * std::f64::consts::PI * 2.0).sin();
        let noise = (local_rng.gen::<f64>() - 0.5) * 20.0;

        samples.push((value + baseline + noise) as i16);
    }

    samples
}

/// 封装 HL7 帧（VT + 内容 + CR + FS + CR）
fn wrap_hl7_frame(content: &str) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.push(0x0B); // VT
    frame.extend_from_slice(content.as_bytes());
    frame.push(0x1C); // FS
    frame.push(0x0D); // CR
    frame
}

/// 构建波形 HL7 消息
fn build_wave_hl7(bed_no: u16, vendor: &str, samples: &[i16]) -> String {
    let now = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let msg_id = format!("W{}{}", bed_no, now);
    let sending_app = match vendor.to_lowercase().as_str() {
        "comen" => "NDMonitor",
        _ => "MINDRAY_N_SERIES",
    };

    let sample_str: Vec<String> = samples.iter().map(|s| s.to_string()).collect();
    let samples_joined = sample_str.join("\\S\\");

    format!(
        "MSH|^~\\&|{sending_app}|BED_{bed}|MONITOR|ICU|{ts}|ORU^R01|{msgid}|P|2.6\r\n\
         PID|1||P{bed:04}||Patient_{bed}\r\n\
         OBR|1||WAVE_{bed}|||{ts}|||||||||||{ts}\r\n\
         OBX|1|NM|MDC_ECG_ELEC_POTL_II||{samples}||uV|||||F|||{ts}",
        sending_app = sending_app,
        bed = bed_no,
        ts = now,
        msgid = msg_id,
        samples = samples_joined,
    )
}

/// 构建体征 HL7 消息
fn build_vital_hl7(
    bed_no: u16,
    vendor: &str,
    hr: i32,
    spo2: i32,
    resp: i32,
    sys: i32,
    dia: i32,
) -> String {
    let now = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let msg_id = format!("V{}{}", bed_no, now);
    let sending_app = match vendor.to_lowercase().as_str() {
        "comen" => "NDMonitor",
        _ => "MINDRAY_N_SERIES",
    };

    format!(
        "MSH|^~\\&|{sending_app}|BED_{bed}|MONITOR|ICU|{ts}|ORU^R01|{msgid}|P|2.6\r\n\
         PID|1||P{bed:04}||Patient_{bed}\r\n\
         OBR|1||VITAL_{bed}|||{ts}|||||||||||{ts}\r\n\
         OBX|1|NM|MDC_ECG_HEART_RATE||{hr}|bpm|||||F|||{ts}\r\n\
         OBX|2|NM|MDC_PULS_OXIM_SAT_O2||{spo2}|%|||||F|||{ts}\r\n\
         OBX|3|NM|MDC_RESP_RATE||{resp}|rpm|||||F|||{ts}\r\n\
         OBX|4|NM|MDC_ART_SYS||{sys}|mmHg|||||F|||{ts}\r\n\
         OBX|5|NM|MDC_ART_DIA||{dia}|mmHg|||||F|||{ts}",
        sending_app = sending_app,
        bed = bed_no,
        ts = now,
        msgid = msg_id,
        hr = hr,
        spo2 = spo2,
        resp = resp,
        sys = sys,
        dia = dia,
    )
}

/// 构建报警 HL7 消息
fn build_alarm_hl7(
    bed_no: u16,
    vendor: &str,
    code: &str,
    message: &str,
    priority: &str,
    phase: &str,
    value: i32,
    threshold: i32,
) -> String {
    let now = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let msg_id = format!("A{}{}", bed_no, now);
    let sending_app = match vendor.to_lowercase().as_str() {
        "comen" => "NDMonitor",
        _ => "MINDRAY_N_SERIES",
    };

    format!(
        "MSH|^~\\&|{sending_app}|BED_{bed}|MONITOR|ICU|{ts}|ORU^R40|{msgid}|P|2.6\r\n\
         PID|1||P{bed:04}||Patient_{bed}\r\n\
         OBR|1||ALARM_{bed}|||{ts}|||||||||||{ts}\r\n\
         OBX|1|ST|{code}||{msg}|{prio}|||{phase}|||F|||{ts}\r\n\
         OBX|2|NM|TRIGGER_VALUE||{val}||||||F|||{ts}\r\n\
         OBX|3|NM|THRESHOLD_HIGH||{thr}||||||F|||{ts}",
        sending_app = sending_app,
        bed = bed_no,
        ts = now,
        msgid = msg_id,
        code = code,
        msg = message,
        prio = priority,
        phase = phase,
        val = value,
        thr = threshold,
    )
}
