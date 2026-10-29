//! TCP 接入服务（单机版）
//!
//! 接收 HL7 设备的 TCP 连接，解析协议后直接发布到消息总线。

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use bytes::BytesMut;
use monitor_common::metrics::health_server::HealthProvider;
use monitor_common::metrics::{AccessMetrics, ComponentHealth, HealthCheck, HealthStatus};
use monitor_proto::frame::Hl7FrameParser;
use monitor_proto::parser::Hl7Parser;
use tokio::{
    io::{AsyncReadExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tracing::{error, info, warn};

use crate::bus::SharedMessageBus;

const MAX_BEDS: u16 = 16;

/// 床位映射管理器
pub struct BedManager {
    ip_to_bed: HashMap<String, u16>,
    config_mapping: HashMap<String, u16>,
    available_beds: Vec<u16>,
    next_bed: u16,
}

impl BedManager {
    pub fn new(config_mapping: HashMap<String, u16>) -> Self {
        let mut next_bed = 1;
        for (&bed_no) in config_mapping.values() {
            if bed_no >= next_bed {
                next_bed = bed_no + 1;
            }
        }

        let mut available_beds = Vec::new();
        for bed in 1..=MAX_BEDS {
            if !config_mapping.values().any(|&mapped_bed| mapped_bed == bed) {
                available_beds.push(bed);
            }
        }

        Self {
            ip_to_bed: HashMap::new(),
            config_mapping,
            available_beds,
            next_bed,
        }
    }

    pub fn get_or_assign_bed(&mut self, device_ip: &str) -> u16 {
        if let Some(&bed_no) = self.config_mapping.get(device_ip) {
            return bed_no;
        }

        if let Some(&bed_no) = self.ip_to_bed.get(device_ip) {
            return bed_no;
        }

        if let Some(bed_no) = self.available_beds.pop() {
            self.ip_to_bed.insert(device_ip.to_string(), bed_no);
            info!(device_ip = %device_ip, bed_no = bed_no, "分配床位号");
            bed_no
        } else if self.next_bed <= MAX_BEDS {
            let bed_no = self.next_bed;
            self.ip_to_bed.insert(device_ip.to_string(), bed_no);
            self.next_bed += 1;
            info!(device_ip = %device_ip, bed_no = bed_no, "分配床位号");
            bed_no
        } else {
            warn!(device_ip = %device_ip, "床位已用完，使用默认床位号 1");
            1
        }
    }

    pub fn release_bed(&mut self, device_ip: &str) {
        if let Some(bed_no) = self.ip_to_bed.remove(device_ip) {
            if !self.config_mapping.values().any(|&mapped_bed| mapped_bed == bed_no) {
                self.available_beds.push(bed_no);
                self.available_beds.sort();
            }
            info!(device_ip = %device_ip, bed_no = bed_no, "释放床位号");
        }
    }
}

/// TCP 接入服务
pub struct TcpAccessService {
    listen_addr: String,
    bus: SharedMessageBus,
    metrics: Arc<AccessMetrics>,
    max_connections: usize,
    bed_manager: Arc<Mutex<BedManager>>,
}

impl TcpAccessService {
    /// 创建新的接入服务
    pub fn new(
        listen_addr: String,
        bus: SharedMessageBus,
        metrics: Arc<AccessMetrics>,
        max_connections: usize,
        ip_to_bed: HashMap<String, u16>,
    ) -> Self {
        Self {
            listen_addr,
            bus,
            metrics,
            max_connections,
            bed_manager: Arc::new(Mutex::new(BedManager::new(ip_to_bed))),
        }
    }

    /// 启动服务
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!("TCP 接入服务已启动: {}", self.listen_addr);

        let active_count = Arc::new(Mutex::new(0usize));

        loop {
            let (stream, addr) = listener.accept().await?;

            // 检查连接数限制
            {
                let count = active_count.lock().await;
                if *count >= self.max_connections {
                    warn!(
                        client = %addr,
                        "连接数已达上限 ({})，拒绝连接",
                        self.max_connections
                    );
                    drop(stream);
                    continue;
                }
            }

            self.metrics.connections_total.inc();
            self.metrics.connections_active.inc();

            let bus = self.bus.clone();
            let metrics = self.metrics.clone();
            let count_clone = active_count.clone();
            let bed_manager = self.bed_manager.clone();

            {
                let mut count = count_clone.lock().await;
                *count += 1;
            }

            tokio::spawn(async move {
                info!(client = %addr, "设备已连接");

                let result = handle_connection(stream, bus, metrics.clone(), bed_manager.clone()).await;

                // 连接断开
                metrics.connections_active.dec();
                {
                    let mut count = count_clone.lock().await;
                    if *count > 0 {
                        *count -= 1;
                    }
                }

                if let Err(e) = result {
                    warn!(client = %addr, error = %e, "连接断开");
                }

                info!(client = %addr, "设备已断开");
            });
        }
    }
}

/// 处理单个设备连接
async fn handle_connection(
    stream: TcpStream,
    bus: SharedMessageBus,
    metrics: Arc<AccessMetrics>,
    bed_manager: Arc<Mutex<BedManager>>,
) -> Result<()> {
    let addr = stream.peer_addr()?;
    let device_ip = addr.ip().to_string();

    let bed_no = {
        let mut bm = bed_manager.lock().await;
        bm.get_or_assign_bed(&device_ip)
    };

    let mut reader = BufReader::new(stream);
    let mut frame_parser = Hl7FrameParser::new();
    let hl7_parser = Hl7Parser::new();
    let mut buffer = BytesMut::with_capacity(8192);

    loop {
        let n = match reader.read_buf(&mut buffer).await {
            Ok(n) => n,
            Err(e) => {
                warn!(client = %addr, error = %e, "读取数据失败");
                break;
            }
        };
        if n == 0 {
            break;
        }

        metrics.frames_received.inc();
        metrics.bytes_received.add(n as u64);

        frame_parser.feed(&buffer);
        buffer.clear();

        while let Ok(Some(frame_data)) = frame_parser.next_frame() {
            let content = match String::from_utf8(frame_data.clone()) {
                Ok(s) => s,
                Err(e) => {
                    metrics.parse_errors.inc();
                    warn!(client = %addr, error = %e, "帧内容非 UTF-8");
                    continue;
                }
            };

            // 调试：记录原始报文前100字符
            tracing::debug!(client = %addr, raw = %&content[..content.len().min(100)], "收到原始报文");

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hl7_parser.parse(&content, &device_ip)
            }));

            match result {
                Ok(Ok(mut parsed)) => {
                    metrics.messages_parsed.inc();
                    set_bed_no(&mut parsed, bed_no);

                    // 调试：记录解析结果类型
                    use monitor_proto::parser::ParsedData;
                    match &parsed {
                        ParsedData::VitalSigns(v) => {
                            tracing::info!(client = %addr, bed_no = %bed_no, hr = ?v.heart_rate, spo2 = ?v.spo2, nibp_sys = ?v.nibp_sys, nibp_dia = ?v.nibp_dia, nibp_map = ?v.nibp_map, pi = ?v.pi, "收到体征数据");
                        }
                        ParsedData::Waveform(waves) => {
                            tracing::info!(client = %addr, bed_no = %bed_no, count = %waves.len(), "收到波形数据");
                        }
                        ParsedData::Alarm(a) => {
                            tracing::debug!(client = %addr, bed_no = %bed_no, code = %a.alarm_code, "收到报警数据");
                        }
                    }

                    let process_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_parsed_data(&parsed, &bus, &metrics);
                    }));
                    if let Err(_) = process_result {
                        error!(client = %addr, "处理解析数据时发生 panic，关闭连接");
                        metrics.parse_errors.inc();
                        break;
                    }
                }
                Ok(Err(e)) => {
                    metrics.parse_errors.inc();
                    warn!(client = %addr, error = %e, raw = %&content[..content.len().min(200)], "解析 HL7 消息失败");
                }
                Err(_) => {
                    metrics.parse_errors.inc();
                    error!(client = %addr, "解析 HL7 消息时发生 panic，关闭连接");
                    break;
                }
            }
        }
    }

    let mut bm = bed_manager.lock().await;
    bm.release_bed(&device_ip);

    Ok(())
}

fn set_bed_no(parsed: &mut monitor_proto::parser::ParsedData, bed_no: u16) {
    use monitor_proto::parser::ParsedData;

    match parsed {
        ParsedData::VitalSigns(vital) => {
            vital.device.bed_no = bed_no;
        }
        ParsedData::Waveform(waves) => {
            for wave in waves {
                wave.device.bed_no = bed_no;
            }
        }
        ParsedData::Alarm(alarm) => {
            alarm.device.bed_no = bed_no;
        }
    }
}

fn process_parsed_data(
    parsed: &monitor_proto::parser::ParsedData,
    bus: &SharedMessageBus,
    metrics: &Arc<AccessMetrics>,
) {
    use monitor_proto::parser::ParsedData;

    match parsed {
        ParsedData::VitalSigns(vital) => {
            if let Err(e) = bus.publish_vital(vital.clone()) {
                error!(error = %e, "发布体征消息失败");
                metrics.mq_send_errors.inc();
            } else {
                metrics.mq_sent.inc();
            }
        }
        ParsedData::Waveform(waves) => {
            for wave in waves {
                if let Err(e) = bus.publish_waveform(wave.clone()) {
                    error!(error = %e, "发布波形消息失败");
                    metrics.mq_send_errors.inc();
                } else {
                    metrics.mq_sent.inc();
                }
            }
        }
        ParsedData::Alarm(alarm) => {
            if let Err(e) = bus.publish_alarm(alarm.clone()) {
                error!(error = %e, "发布报警消息失败");
                metrics.mq_send_errors.inc();
            } else {
                metrics.mq_sent.inc();
            }
        }
    }
}

// HealthProvider 实现
pub struct AccessHealthProvider {
    metrics: Arc<AccessMetrics>,
}

impl AccessHealthProvider {
    pub fn new(metrics: Arc<AccessMetrics>) -> Self {
        Self { metrics }
    }
}

impl HealthProvider for AccessHealthProvider {
    fn health(&self) -> HealthCheck {
        HealthCheck {
            service: "access".to_string(),
            status: HealthStatus::Healthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.metrics.start_time.elapsed().as_secs(),
            components: vec![
                ComponentHealth {
                    name: "connections".to_string(),
                    status: HealthStatus::Healthy,
                    detail: Some(format!("active: {}, total: {}",
                        self.metrics.connections_active.get(),
                        self.metrics.connections_total.get()
                    )),
                },
                ComponentHealth {
                    name: "messages".to_string(),
                    status: HealthStatus::Healthy,
                    detail: Some(format!("parsed: {}, errors: {}",
                        self.metrics.messages_parsed.get(),
                        self.metrics.parse_errors.get()
                    )),
                },
            ],
        }
    }
}
