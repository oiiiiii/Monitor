//! HL7 帧边界处理
//!
//! 负责从字节流中识别 HL7 帧的起始和结束边界，
//! 处理粘包、拆包、半帧缓存等问题。

use bytes::BytesMut;
use monitor_common::error::MonitorResult;
use monitor_common::utils::{HL7_FRAME_HEADER, HL7_FRAME_TAIL_1, HL7_FRAME_TAIL_2};

/// HL7 帧解析器
///
/// 从字节流中逐步提取完整的 HL7 帧，
/// 内部维护未完成帧的缓存。
pub struct Hl7FrameParser {
    /// 内部缓冲区，存储尚未解析的字节
    buffer: BytesMut,
    /// 是否正在帧内（已找到帧头，等待帧尾）
    in_frame: bool,
}

impl Hl7FrameParser {
    /// 创建新的帧解析器
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(8192),
            in_frame: false,
        }
    }

    /// 向解析器喂入新的数据
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// 尝试提取下一个完整的 HL7 帧
    ///
    /// 返回 `Ok(Some(frame))` 表示提取到一帧，
    /// 返回 `Ok(None)` 表示数据不足，需要等待更多数据。
    pub fn next_frame(&mut self) -> MonitorResult<Option<Vec<u8>>> {
        loop {
            if self.buffer.is_empty() {
                return Ok(None);
            }

            if !self.in_frame {
                // 寻找帧头
                if let Some(pos) = find_byte(&self.buffer, HL7_FRAME_HEADER) {
                    // 丢弃帧头之前的数据
                    let _ = self.buffer.split_to(pos + 1);
                    self.in_frame = true;
                } else {
                    // 没找到帧头，清空缓冲区（都是垃圾数据）
                    self.buffer.clear();
                    return Ok(None);
                }
            }

            if self.in_frame {
                // 寻找帧尾（\x1C\x0D）
                if let Some(pos) = find_frame_tail(&self.buffer) {
                    // 提取帧内容（不包含帧头和帧尾）
                    let frame_data = self.buffer[..pos].to_vec();
                    // 跳过帧尾
                    let _ = self.buffer.split_to(pos + 2);
                    self.in_frame = false;
                    return Ok(Some(frame_data));
                } else {
                    // 没找到帧尾，需要更多数据
                    return Ok(None);
                }
            }
        }
    }

    /// 清空内部缓冲区
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.in_frame = false;
    }

    /// 获取当前缓冲区大小
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for Hl7FrameParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 在字节切片中查找指定字节
fn find_byte(data: &[u8], byte: u8) -> Option<usize> {
    data.iter().position(|&b| b == byte)
}

/// 查找 HL7 帧尾（\x1C\x0D）
fn find_frame_tail(data: &[u8]) -> Option<usize> {
    if data.len() < 2 {
        return None;
    }
    for i in 0..data.len() - 1 {
        if data[i] == HL7_FRAME_TAIL_1 && data[i + 1] == HL7_FRAME_TAIL_2 {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_complete_frame() {
        let mut parser = Hl7FrameParser::new();
        let frame = b"MSH|^~\\&|test|||20260101||ORU^R01|1|P|2.6";
        let mut data = vec![HL7_FRAME_HEADER];
        data.extend_from_slice(frame);
        data.push(HL7_FRAME_TAIL_1);
        data.push(HL7_FRAME_TAIL_2);

        parser.feed(&data);
        let result = parser.next_frame().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), frame);
    }

    #[test]
    fn test_partial_frame() {
        let mut parser = Hl7FrameParser::new();
        let frame = b"MSH|^~\\&|test|||20260101||ORU^R01|1|P|2.6";

        // 先喂一半数据
        let mut data1 = vec![HL7_FRAME_HEADER];
        data1.extend_from_slice(&frame[..10]);
        parser.feed(&data1);
        assert!(parser.next_frame().unwrap().is_none());

        // 再喂剩下的数据 + 帧尾
        let mut data2 = Vec::from(&frame[10..]);
        data2.push(HL7_FRAME_TAIL_1);
        data2.push(HL7_FRAME_TAIL_2);
        parser.feed(&data2);

        let result = parser.next_frame().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), frame);
    }

    #[test]
    fn test_multiple_frames() {
        let mut parser = Hl7FrameParser::new();
        let frame1 = b"MSH|^~\\&|test1|||20260101||ORU^R01|1|P|2.6";
        let frame2 = b"MSH|^~\\&|test2|||20260101||ORU^R01|2|P|2.6";

        let mut data = Vec::new();
        data.push(HL7_FRAME_HEADER);
        data.extend_from_slice(frame1);
        data.push(HL7_FRAME_TAIL_1);
        data.push(HL7_FRAME_TAIL_2);
        data.push(HL7_FRAME_HEADER);
        data.extend_from_slice(frame2);
        data.push(HL7_FRAME_TAIL_1);
        data.push(HL7_FRAME_TAIL_2);

        parser.feed(&data);

        let result1 = parser.next_frame().unwrap();
        assert!(result1.is_some());
        assert_eq!(result1.unwrap(), frame1);

        let result2 = parser.next_frame().unwrap();
        assert!(result2.is_some());
        assert_eq!(result2.unwrap(), frame2);
    }

    #[test]
    fn test_garbage_before_frame() {
        let mut parser = Hl7FrameParser::new();
        let frame = b"MSH|^~\\&|test|||20260101||ORU^R01|1|P|2.6";

        let mut data = b"garbage data here".to_vec();
        data.push(HL7_FRAME_HEADER);
        data.extend_from_slice(frame);
        data.push(HL7_FRAME_TAIL_1);
        data.push(HL7_FRAME_TAIL_2);

        parser.feed(&data);
        let result = parser.next_frame().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), frame);
    }
}
