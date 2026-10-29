//! Redis 缓存实现
//!
//! 热数据缓存：
//! - 最新生命体征（Hash）
//! - 最近 10 秒波形（List，固定长度环形列表）
//! - 活跃报警（Hash / Set）
//! - 设备在线状态（Hash）

use std::time::Duration;

use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client, RedisResult};
use tracing::{error, info, warn};

use crate::types::{AlarmEvent, AlarmPriority, VitalSignsData, WaveformData};

/// Redis 配置
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// 主机地址
    pub host: String,
    /// 端口
    pub port: u16,
    /// 密码
    pub password: Option<String>,
    /// 数据库编号
    pub db: i64,
    /// 连接超时（秒）
    pub timeout_secs: u64,
    /// 波形缓存长度（秒数）
    pub wave_cache_seconds: u32,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            db: 0,
            timeout_secs: 5,
            wave_cache_seconds: 10,
        }
    }
}

impl RedisConfig {
    /// 生成 Redis URL
    pub fn url(&self) -> String {
        if let Some(pwd) = &self.password {
            format!("redis://:{}@{}:{}/{}", pwd, self.host, self.port, self.db)
        } else {
            format!("redis://{}:{}/{}", self.host, self.port, self.db)
        }
    }
}

/// Redis 缓存管理器
#[derive(Clone)]
pub struct RedisCache {
    client: Client,
    config: RedisConfig,
}

impl RedisCache {
    /// 创建新的 Redis 缓存
    pub fn new(config: RedisConfig) -> anyhow::Result<Self> {
        let client = Client::open(config.url())?;
        Ok(Self { client, config })
    }

    /// 获取异步连接
    async fn get_conn(&self) -> anyhow::Result<MultiplexedConnection> {
        let conn = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            self.client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Redis 连接超时"))??;
        Ok(conn)
    }

    /// 测试连接
    pub async fn ping(&self) -> bool {
        match self.get_conn().await {
            Ok(mut conn) => {
                let result: RedisResult<String> = redis::cmd("PING").query_async(&mut conn).await;
                result.is_ok()
            }
            Err(_) => false,
        }
    }

    /// 初始化（测试连接）
    pub async fn init(&self) -> anyhow::Result<()> {
        info!(host = %self.config.host, port = self.config.port, "连接 Redis");
        if self.ping().await {
            info!("Redis 连接成功");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Redis 连接失败"))
        }
    }

    // ============================================================
    // 生命体征缓存
    // ============================================================

    /// 更新最新生命体征
    pub async fn update_vital_signs(&self, vital: &VitalSignsData) -> anyhow::Result<()> {
        let key = format!("vital:latest:{}", vital.device.bed_no);
        let data = serde_json::to_string(vital)?;

        let mut conn = self.get_conn().await?;
        let _: () = conn.set(&key, data).await?;
        // 设置过期时间（60 秒无数据则认为设备离线）
        let _: () = conn.expire(&key, 60).await?;

        Ok(())
    }

    /// 获取最新生命体征
    pub async fn get_vital_signs(&self, bed_no: u16) -> anyhow::Result<Option<VitalSignsData>> {
        let key = format!("vital:latest:{}", bed_no);

        let mut conn = self.get_conn().await?;
        let data: Option<String> = conn.get(&key).await?;

        match data {
            Some(s) => {
                let vital: VitalSignsData = serde_json::from_str(&s)?;
                Ok(Some(vital))
            }
            None => Ok(None),
        }
    }

    // ============================================================
    // 波形缓存
    // ============================================================

    /// 追加波形数据到缓存（固定长度环形列表）
    pub async fn append_waveform(&self, wave: &WaveformData) -> anyhow::Result<()> {
        let key = format!(
            "wave:recent:{}:{}",
            wave.device.bed_no,
            wave.lead.name().to_lowercase()
        );

        let data = serde_json::to_string(wave)?;

        let mut conn = self.get_conn().await?;

        // 推入列表左侧
        let _: () = conn.lpush(&key, &data).await?;

        // 裁剪列表，保留足够的条目（按采样率估算）
        // 比如 500Hz * 10秒 = 5000个采样点，假设每次推送约 100 个采样点
        // 保守估计保留 200 条
        let max_len = (self.config.wave_cache_seconds * 20) as isize;
        let _: () = conn.ltrim(&key, 0, max_len - 1).await?;

        // 设置过期时间
        let _: () = conn.expire(&key, 60).await?;

        Ok(())
    }

    /// 获取最近的波形数据
    pub async fn get_recent_waveforms(
        &self,
        bed_no: u16,
        lead_type: &str,
        count: usize,
    ) -> anyhow::Result<Vec<WaveformData>> {
        let key = format!(
            "wave:recent:{}:{}",
            bed_no,
            lead_type.to_lowercase().replace(" ", "_")
        );

        let mut conn = self.get_conn().await?;
        let items: Vec<String> = conn.lrange(&key, 0, count as isize - 1).await?;

        let mut waves = Vec::with_capacity(items.len());
        for item in items {
            match serde_json::from_str::<WaveformData>(&item) {
                Ok(w) => waves.push(w),
                Err(e) => warn!(error = %e, "波形数据反序列化失败"),
            }
        }

        Ok(waves)
    }

    // ============================================================
    // 报警缓存
    // ============================================================

    /// 更新活跃报警
    pub async fn update_alarm(&self, alarm: &AlarmEvent) -> anyhow::Result<()> {
        let bed_key = format!("alarm:active:{}", alarm.device.bed_no);
        let alarm_key = format!("alarm:detail:{}:{}", alarm.device.bed_no, alarm.alarm_code);

        let mut conn = self.get_conn().await?;

        let is_start = alarm.event_phase.eq_ignore_ascii_case("start")
            || alarm.alarm_state.eq_ignore_ascii_case("active");

        if is_start {
            // 报警开始：加入活跃集合 + 写入详情
            let data = serde_json::to_string(alarm)?;
            let _: () = conn.set(&alarm_key, data).await?;
            let _: () = conn.expire(&alarm_key, 3600).await?; // 1 小时过期

            // 加入有序集合（score 用优先级权重：H=3, PM=2, PL=1）
            let score = match alarm.priority {
                AlarmPriority::H => 3,
                AlarmPriority::PM => 2,
                AlarmPriority::PL => 1,
            };
            let _: () = conn
                .zadd(&bed_key, alarm.alarm_code.clone(), score)
                .await?;

            // 设置过期
            let _: () = conn.expire(&bed_key, 3600).await?;
        } else {
            // 报警结束：从活跃集合移除 + 删除详情
            let _: () = conn.zrem(&bed_key, &alarm.alarm_code).await?;
            let _: () = conn.del(&alarm_key).await?;
        }

        Ok(())
    }

    /// 获取床位的活跃报警列表
    pub async fn get_active_alarms(&self, bed_no: u16) -> anyhow::Result<Vec<AlarmEvent>> {
        let bed_key = format!("alarm:active:{}", bed_no);

        let mut conn = self.get_conn().await?;

        // 获取所有报警码（按优先级倒序）
        let alarm_codes: Vec<String> = conn.zrevrange(&bed_key, 0, -1).await?;

        let mut alarms = Vec::new();
        for code in &alarm_codes {
            let detail_key = format!("alarm:detail:{}:{}", bed_no, code);
            let data: Option<String> = conn.get(&detail_key).await?;
            if let Some(s) = data {
                match serde_json::from_str::<AlarmEvent>(&s) {
                    Ok(a) => alarms.push(a),
                    Err(e) => warn!(error = %e, "报警数据反序列化失败"),
                }
            }
        }

        Ok(alarms)
    }

    // ============================================================
    // 设备状态缓存
    // ============================================================

    /// 更新设备在线状态
    pub async fn update_device_online(
        &self,
        device_ip: &str,
        bed_no: u16,
        is_online: bool,
    ) -> anyhow::Result<()> {
        let key = "device:status".to_string();
        let value = format!("{}:{}:{}", bed_no, if is_online { 1 } else { 0 }, device_ip);

        let mut conn = self.get_conn().await?;
        let _: () = conn.hset(&key, device_ip, value).await?;
        Ok(())
    }

    /// 获取所有设备状态（床位号 -> 是否在线）
    pub async fn get_all_device_status(&self) -> anyhow::Result<std::collections::HashMap<u16, bool>> {
        let mut conn = self.get_conn().await?;
        let values: std::collections::HashMap<String, String> =
            conn.hgetall("device:status").await?;

        let mut result = std::collections::HashMap::new();
        for (_, value) in values {
            let parts: Vec<&str> = value.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(bed_no) = parts[0].parse::<u16>() {
                    let online = parts[1] == "1";
                    result.insert(bed_no, online);
                }
            }
        }

        Ok(result)
    }

    // ============================================================
    // 工具方法
    // ============================================================

    /// 获取配置
    pub fn config(&self) -> &RedisConfig {
        &self.config
    }
}
