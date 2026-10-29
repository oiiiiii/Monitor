//! MDC 编码映射表
//!
//! 包含常用的 MDC 代码、迈瑞私有编码（99MNDRY）、
//! 单位编码等映射关系。

use std::collections::HashMap;
use std::sync::LazyLock;

/// MDC 代码到参数名称的映射（生命体征）
pub static MDC_VITAL_PARAMS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // 心率
    m.insert("147842", "MDC_ECG_HEART_RATE");

    // 呼吸率
    m.insert("151578", "MDC_TTHOR_RESP_RATE");
    m.insert("151562", "MDC_TTHOR_RESP_RATE_OLD"); // 科曼使用

    // 血氧饱和度
    m.insert("150456", "MDC_PULS_OXIM_SAT_O2");

    // 脉搏率
    m.insert("149530", "MDC_PULS_OXIM_PULS_RATE");
    m.insert("149546", "MDC_PULS_RATE_NON_INV"); // 科曼使用

    // 灌注指数
    m.insert("150488", "MDC_BLD_PERF_INDEX");

    // 无创血压
    m.insert("150021", "MDC_PRESS_BLD_NONINV_SYS");
    m.insert("150022", "MDC_PRESS_BLD_NONINV_DIA");
    m.insert("150023", "MDC_PRESS_BLD_NONINV_MEAN");

    // 有创血压
    m.insert("150037", "MDC_PRESS_BLD_ART_ABP_SYS");
    m.insert("150038", "MDC_PRESS_BLD_ART_ABP_DIA");
    m.insert("150039", "MDC_PRESS_BLD_ART_ABP_MEAN");
    m.insert("150033", "MDC_PRESS_BLD_ART_ABP_SYS_OLD"); // 科曼使用
    m.insert("150034", "MDC_PRESS_BLD_ART_ABP_DIA_OLD"); // 科曼使用

    // 心律失常参数
    m.insert("148066", "MDC_ECG_V_P_C_RATE"); // 室早率

    m
});

/// 迈瑞私有编码映射（99MNDRY 命名空间）
pub static MNDRY_VITAL_PARAMS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // 血压脉搏率
    m.insert("364", "MNDRY_BLD_PULS_RATE_ART_ABP");

    // 心律失常参数
    m.insert("108", "MNDRY_ECG_PAUSE_RATE"); // 停搏率
    m.insert("352", "MNDRY_ECG_VPB_RATE"); // 室早二联率
    m.insert("298", "MNDRY_ECG_COUPLETS_RATE"); // 成对室早率
    m.insert("299", "MNDRY_ECG_MISSED_BEATS_RATE"); // 漏搏率
    m.insert("302", "MNDRY_ECG_P_V_C_RonT_RATE"); // RonT 室早率

    m
});

/// MDC 导联代码映射
pub static MDC_LEAD_CODES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("131329", "MDC_ECG_ELEC_POTL_I");
    m.insert("131330", "MDC_ECG_ELEC_POTL_II");
    m.insert("131389", "MDC_ECG_ELEC_POTL_III");
    m.insert("131390", "MDC_ECG_ELEC_POTL_AVR");
    m.insert("131391", "MDC_ECG_ELEC_POTL_AVL");
    m.insert("131392", "MDC_ECG_ELEC_POTL_AVF");
    m.insert("131395", "MDC_ECG_ELEC_POTL_V");
    m.insert("131396", "MDC_ECG_ELEC_POTL_V1");
    m.insert("131397", "MDC_ECG_ELEC_POTL_V2");
    m.insert("131398", "MDC_ECG_ELEC_POTL_V3");
    m.insert("131399", "MDC_ECG_ELEC_POTL_V4");
    m.insert("131400", "MDC_ECG_ELEC_POTL_V5");
    m.insert("131401", "MDC_ECG_ELEC_POTL_V6");
    m.insert("150032", "MDC_PRESS_BLD_ART");
    m.insert("150452", "MDC_PULS_OXIM_PLETH");
    m.insert("151780", "MDC_IMPED_TTHOR");
    m
});

/// 采样率 MDC 代码（兼容不同固件版本）
pub static SAMPLE_RATE_CODES: &[&str] = &[
    "0",          // 迈瑞 10.10.12.66 使用
    "68320",      // 迈瑞 10.10.12.65 使用 MDC_ATTR_SAMPLE_RATE
];

/// INOP（导联不可操作）MDC 代码（兼容不同固件版本）
pub static INOP_CODES: &[&str] = &[
    "262196",     // 迈瑞 10.10.12.66 使用
    "196660",     // 迈瑞 10.10.12.65 使用
];

/// 报警相关 MDC 代码
pub static ALARM_CODES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // 报警事件
    m.insert("196616", "MDC_EVT_ALARM");

    // 报警类型
    m.insert("196674", "MDC_EVT_LO_VAL_LT_LIM"); // 低值超限
    m.insert("196675", "MDC_EVT_HI_VAL_GT_LIM"); // 高值超限
    m.insert("196680", "MDC_EVT_LEAD_OFF"); // 导联脱落
    m.insert("196681", "MDC_EVT_NOISY"); // 噪声干扰
    m.insert("196682", "MDC_EVT_STAT_OFF"); // 状态关闭

    // 报警属性
    m.insert("68480", "MDC_ATTR_ALERT_SOURCE"); // 报警来源
    m.insert("68481", "MDC_ATTR_EVENT_PHASE"); // 事件阶段
    m.insert("68482", "MDC_ATTR_ALARM_STATE"); // 报警状态
    m.insert("68483", "MDC_ATTR_ALARM_INACTIVATION_STATE"); // 报警消除状态
    m.insert("68484", "MDC_ATTR_ALARM_PRIORITY"); // 报警优先级
    m.insert("68485", "MDC_ATTR_ALERT_TYPE"); // 报警类型

    // 分辨率
    m.insert("2327", "MDC_ATTR_NU_MSMT_RES"); // 测量分辨率

    m
});

/// 单位 MDC 代码映射
pub static UNIT_CODES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("264864", "MDC_DIM_BEAT_PER_MIN"); // 次/分
    m.insert("264928", "MDC_DIM_RESP_PER_MIN"); // 呼吸次/分
    m.insert("266016", "MDC_DIM_MMHG"); // mmHg
    m.insert("262688", "MDC_DIM_PERCENT"); // %
    m.insert("262656", "MDC_DIM_DIMLESS"); // 无量纲
    m.insert("266418", "MDC_DIM_MILLI_VOLT"); // 毫伏
    m.insert("264608", "MDC_DIM_PER_SEC"); // 每秒（采样率）
    m.insert("264640", "MDC_DIM_HZ"); // 赫兹
    m
});

/// 设备厂商识别关键字
pub mod vendor {
    /// 科曼设备识别关键字
    pub const COMEN_KEYWORDS: &[&str] = &["NDMonitor", "COMEN", "99COMEN"];

    /// 迈瑞设备识别关键字
    pub const MINDRAY_KEYWORDS: &[&str] = &["MINDRAY_N_SERIES", "MINDRAY", "99MNDRY"];
}

/// 波形 OBR 观测标识关键字
pub mod wave_obr {
    /// 科曼波形标识
    pub const COMEN_WAVE: &[&str] = &["MON_WAVE", "99COMEN"];

    /// 迈瑞波形标识
    pub const MINDRAY_WAVE: &[&str] = &["CONTINUOUS WAVEFORM", "100001", "99MNDRY"];
}

/// 生命体征 OBR 观测标识
pub const VITAL_OBR_CODE: &str = "182777000"; // monitoring of patient

/// 报警 OBR 观测标识
pub const ALARM_OBR_CODE: &str = "196616"; // MDC_EVT_ALARM

/// 判断是否为采样率 OBX
pub fn is_sample_rate_obx(code: &str, name_system: &str) -> bool {
    // 检查代码是否在采样率代码列表中
    if SAMPLE_RATE_CODES.contains(&code) {
        return true;
    }
    // 检查名称中是否包含 SAMP_RATE 或 SAMPLE_RATE
    let name_upper = code.to_uppercase();
    name_upper.contains("SAMP_RATE") || name_upper.contains("SAMPLE_RATE")
}

/// 判断是否为 INOP OBX
pub fn is_inop_obx(code: &str, name: &str) -> bool {
    if INOP_CODES.contains(&code) {
        return true;
    }
    let name_upper = name.to_uppercase();
    name_upper.contains("INOP") || name_upper.contains("MDC_EVT_INOP")
}
