//! 多床位心电监护系统 - HL7 协议解析库
//!
//! 负责解析科曼、迈瑞监护仪的 HL7 v2.6 协议报文，
//! 输出标准化的生命体征、波形数据、报警事件。

pub mod frame;
pub mod parser;
pub mod segments;
pub mod vital;
pub mod waveform;
pub mod alarm;
pub mod mdc_codes;

pub use parser::{Hl7Parser, ParsedData};
pub use vital::VitalSignsParser;
pub use waveform::WaveformParser;
pub use alarm::AlarmParser;
