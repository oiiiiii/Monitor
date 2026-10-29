//! 设备连接处理
//!
//! 负责单个 TCP 连接的生命周期管理：
//! - 读取字节流
//! - 帧边界检测与解析
//! - HL7 消息解析
//! - 事件发送

use std::time::Duration;
use std::time::Instant;

use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use monitor_common::error::MonitorResult;
use monitor_proto::{frame::Hl7FrameParser, Hl7Parser, ParsedData};

use crate::event::{AccessEvent, DisconnectReason};

/// 设备连接处理器
pub struct DeviceConnection {
    /// 设备 IP
    device_ip: String,
    /// 帧解析器
    frame_parser: Hl7FrameParser,
    /// HL7 消息解析器
    hl7_parser: Hl7Parser,
    /// 事件发送通道
    event_tx: mpsc::Sender<AccessEvent>,
    /// 连接建立时间
    connect_time: Instant,
    /// 最后一次收到数据的时间
    last_data_time: Instant,
    /// 连接超时时间
    timeout: Duration,
    /// 接收缓冲区大小
    recv_buffer_size: usize,
}

impl DeviceConnection {
    /// 创建新的设备连接处理器
    pub fn new(
        device_ip: String,
        event_tx: mpsc::Sender<AccessEvent>,
        timeout: Duration,
        recv_buffer_size: usize,
    ) -> Self {
        let now = Instant::now();
        Self {
            device_ip,
            frame_parser: Hl7FrameParser::new(),
            hl7_parser: Hl7Parser::new(),
            event_tx,
            connect_time: now,
            last_data_time: now,
            timeout,
            recv_buffer_size,
        }
    }

    /// 获取设备 IP
    pub fn device_ip(&self) -> &str {
        &self.device_ip
    }

    /// 处理连接的读取循环
    ///
    /// 持续从 TCP 流中读取数据，解析帧和消息，
    /// 并通过事件通道发送解析结果。
    pub async fn run_read_loop(&mut self, reader: OwnedReadHalf) -> DisconnectReason {
        let mut reader = BufReader::with_capacity(self.recv_buffer_size, reader);
        let mut buf = vec![0u8; self.recv_buffer_size];

        loop {
            // 带超时的读取
            let read_result = tokio::time::timeout(
                self.timeout,
                reader.read(&mut buf),
            ).await;

            match read_result {
                Ok(Ok(0)) => {
                    // 连接被对端关闭
                    info!(ip = %self.device_ip, "设备连接正常关闭");
                    return DisconnectReason::Normal;
                }
                Ok(Ok(n)) => {
                    // 成功读取数据
                    self.last_data_time = Instant::now();
                    debug!(ip = %self.device_ip, bytes = n, "收到数据");

                    // 喂给帧解析器
                    if let Err(e) = self.process_data(&buf[..n]).await {
                        error!(ip = %self.device_ip, error = %e, "处理数据失败");
                        self.send_event(AccessEvent::ParseError {
                            device_ip: self.device_ip.clone(),
                            error: e.to_string(),
                        }).await;
                    }
                }
                Ok(Err(e)) => {
                    // 读取错误
                    warn!(ip = %self.device_ip, error = %e, "读取数据出错");
                    return DisconnectReason::NetworkError;
                }
                Err(_) => {
                    // 超时
                    warn!(ip = %self.device_ip, "连接超时，无数据接收");
                    return DisconnectReason::Timeout;
                }
            }
        }
    }

    /// 处理接收到的数据
    async fn process_data(&mut self, data: &[u8]) -> MonitorResult<()> {
        // 喂给帧解析器
        self.frame_parser.feed(data);

        // 循环提取所有完整的帧
        while let Some(frame) = self.frame_parser.next_frame()? {
            // 将帧内容转换为字符串（HL7 是 ASCII/UTF-8 编码）
            let content = match String::from_utf8(frame) {
                Ok(s) => s,
                Err(e) => {
                    warn!(ip = %self.device_ip, error = %e, "帧内容不是有效的 UTF-8");
                    continue;
                }
            };

            debug!(ip = %self.device_ip, len = content.len(), "解析到完整 HL7 帧");

            // 解析 HL7 消息
            match self.hl7_parser.parse(&content, &self.device_ip) {
                Ok(parsed) => {
                    self.handle_parsed_data(parsed).await;
                }
                Err(e) => {
                    warn!(ip = %self.device_ip, error = %e, "HL7 消息解析失败");
                    self.send_event(AccessEvent::ParseError {
                        device_ip: self.device_ip.clone(),
                        error: e.to_string(),
                    }).await;
                }
            }
        }

        Ok(())
    }

    /// 处理解析后的数据
    async fn handle_parsed_data(&self, data: ParsedData) {
        let recv_time = Instant::now();

        let event = match data {
            ParsedData::VitalSigns(vital) => {
                debug!(
                    ip = %self.device_ip,
                    hr = vital.heart_rate.unwrap_or(0),
                    "收到生命体征数据"
                );
                AccessEvent::VitalSignsReceived {
                    device: vital.device.clone(),
                    data: Box::new(vital),
                    recv_time,
                }
            }
            ParsedData::Waveform(waves) => {
                debug!(
                    ip = %self.device_ip,
                    leads = waves.len(),
                    "收到波形数据"
                );
                let device = waves.first().map(|w| w.device.clone()).unwrap_or_default();
                AccessEvent::WaveformReceived {
                    device,
                    data: waves,
                    recv_time,
                }
            }
            ParsedData::Alarm(alarm) => {
                info!(
                    ip = %self.device_ip,
                    code = %alarm.alarm_code,
                    priority = ?alarm.priority,
                    "收到报警事件"
                );
                AccessEvent::AlarmReceived {
                    device: alarm.device.clone(),
                    data: Box::new(alarm),
                    recv_time,
                }
            }
        };

        self.send_event(event).await;
    }

    /// 发送事件
    async fn send_event(&self, event: AccessEvent) {
        if self.event_tx.send(event).await.is_err() {
            error!(ip = %self.device_ip, "事件通道已关闭，无法发送事件");
        }
    }

    /// 获取连接时长
    pub fn connection_duration(&self) -> Duration {
        self.connect_time.elapsed()
    }
}
