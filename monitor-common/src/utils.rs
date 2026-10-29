//! 通用工具函数

/// HL7 帧头
pub const HL7_FRAME_HEADER: u8 = 0x0B; // \x0B

/// HL7 帧尾（两字节）
pub const HL7_FRAME_TAIL_1: u8 = 0x1C; // \x1C
pub const HL7_FRAME_TAIL_2: u8 = 0x0D; // \x0D

/// 默认字段分隔符
pub const DEFAULT_FIELD_SEP: char = '|';

/// 默认组件分隔符
pub const DEFAULT_COMP_SEP: char = '^';

/// 默认重复分隔符
pub const DEFAULT_REPEAT_SEP: char = '~';

/// 默认转义符
pub const DEFAULT_ESCAPE: char = '\\';

/// 默认子组件分隔符
pub const DEFAULT_SUBCOMP_SEP: char = '&';
