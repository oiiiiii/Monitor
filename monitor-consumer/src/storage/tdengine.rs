//! TDengine 时序数据库存储
//!
//! 负责波形数据的持久化存储。
//! 遵循项目规范：
//! - 按「床位 + 导联」建立子表，使用超级表统一 schema
//! - 波形数据必须批量写入，禁止单条循环插入
//! - 配置数据保留策略，自动过期清理历史波形

use std::time::Duration;

use serde::Deserialize;
use tracing::{error, info, warn};

use crate::config::TdengineConfig;
use monitor_common::types::WaveformData;

/// TDengine REST API 响应
#[derive(Debug, Deserialize)]
struct TdResponse {
    code: i32,
    desc: Option<String>,
    #[serde(default)]
    data: Option<Vec<Vec<serde_json::Value>>>,
}

/// TDengine 存储
pub struct TdEngineStore {
    config: TdengineConfig,
    client: reqwest::Client,
    database: String,
}

impl TdEngineStore {
    /// 创建新的 TDengine 存储
    pub fn new(config: TdengineConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("创建 HTTP 客户端失败");

        let database = config.database.clone();

        Self {
            config,
            client,
            database,
        }
    }

    /// 初始化（创建数据库、超级表）
    pub async fn init(&self) -> anyhow::Result<()> {
        info!(host = %self.config.host, port = self.config.port, "初始化 TDengine");

        // 1. 创建数据库
        let create_db_sql = format!(
            "CREATE DATABASE IF NOT EXISTS {} KEEP 365 DURATION 10 BUFFER 16 WAL_FSYNC 1 COMP 1;",
            self.database
        );
        self.execute_sql(&create_db_sql, None).await?;
        info!(database = %self.database, "数据库就绪");

        // 2. 使用数据库
        self.execute_sql(&format!("USE {};", self.database), None).await?;

        // 3. 创建波形超级表
        let create_stable_sql = r#"
            CREATE STABLE IF NOT EXISTS ecg_waveform (
                ts TIMESTAMP,
                sample_value BINARY(10240)
            ) TAGS (
                bed_no INT,
                lead_type NCHAR(20),
                sample_rate INT,
                device_ip NCHAR(50),
                vendor NCHAR(20)
            );
        "#;
        self.execute_sql(create_stable_sql, Some(&self.database)).await?;
        info!("波形超级表创建完成");

        Ok(())
    }

    /// 执行 SQL 语句
    async fn execute_sql(
        &self,
        sql: &str,
        db: Option<&str>,
    ) -> anyhow::Result<TdResponse> {
        let url = format!("{}/rest/sql/{}", self.config.rest_url(), db.unwrap_or(""));

        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .body(sql.to_string())
            .send()
            .await?;

        let result: TdResponse = resp.json().await?;

        if result.code != 0 {
            return Err(anyhow::anyhow!(
                "TDengine 执行失败: code={}, desc={:?}",
                result.code,
                result.desc
            ));
        }

        Ok(result)
    }

    /// 批量写入波形数据
    ///
    /// 按床位 + 导联分组后批量写入
    pub async fn insert_waveforms(
        &self,
        waveforms: &[WaveformData],
    ) -> anyhow::Result<usize> {
        if waveforms.is_empty() {
            return Ok(0);
        }

        let mut count = 0;

        // 按 (床位, 导联) 分组
        use std::collections::HashMap;
        let mut groups: HashMap<(u16, String), Vec<&WaveformData>> = HashMap::new();
        for wave in waveforms {
            let key = (wave.device.bed_no, wave.lead.to_string());
            groups.entry(key).or_insert_with(Vec::new).push(wave);
        }

        // 分组插入
        for ((bed_no, lead_type), waves) in &groups {
            match self.insert_wave_batch(*bed_no, lead_type, waves).await {
                Ok(n) => count += n,
                Err(e) => {
                    error!(
                        bed = bed_no,
                        lead = lead_type,
                        error = %e,
                        "批量写入波形失败"
                    );
                    return Err(e);
                }
            }
        }

        Ok(count)
    }

    /// 批量插入一组波形（同一床位同一导联）
    async fn insert_wave_batch(
        &self,
        bed_no: u16,
        lead_type: &str,
        waves: &[&WaveformData],
    ) -> anyhow::Result<usize> {
        if waves.is_empty() {
            return Ok(0);
        }

        // 取第一条的设备信息作为标签
        let first = waves[0];
        let table_name = format!("wave_bed{}_lead{}", bed_no, lead_type.replace(" ", "_").to_lowercase());

        // 构建批量插入语句（使用 INSERT INTO 使用 INSERT INTO table_name USING super_table_name TAGS (...) VALUES ...）
        let mut sql_values: Vec<String> = waves
            .iter()
            .map(|w| {
                // 将采样值数组编码为 base64 或 hex 存储
                let samples_hex: String = w
                    .samples
                    .iter()
                    .map(|s| format!("{:04x}", s))
                    .collect();
                // 使用时间戳（毫秒）
                let ts_ms = w.start_time.timestamp_millis();
                format!(
                    "({}, '{}')",
                    ts_ms, samples_hex
                )
            })
            .collect();

        // 使用超级表方式插入
        let sql = format!(
            "INSERT INTO {} USING ecg_waveform TAGS({}, '{}', {}, '{}', '{}') VALUES {};",
            table_name,
            bed_no,
            lead_type,
            first.sample_rate,
            first.device.device_ip,
            first.device.vendor.to_string(),
            sql_values.join(" ")
        );

        self.execute_sql(&sql, Some(&self.database)).await?;

        Ok(waves.len())
    }

    /// 查询指定床位指定时间范围的波形
    pub async fn query_waveforms(
        &self,
        bed_no: u16,
        lead_type: &str,
        start_ts: i64,
        end_ts: i64,
    ) -> anyhow::Result<Vec<(i64, Vec<i16>)>> {
        let table_name = format!("wave_bed{}_lead{}", bed_no, lead_type.replace(" ", "_").to_lowercase());

        let sql = format!(
            "SELECT ts, sample_value FROM {} WHERE ts >= {} AND ts <= {} ORDER BY ts ASC;",
            table_name, start_ts, end_ts
        );

        let resp = self.execute_sql(&sql, Some(&self.database)).await?;

        let mut results = Vec::new();

        if let Some(data) = resp.data {
            for row in data {
                if row.len() < 2 {
                    continue;
                }
                let ts = row[0].as_i64().unwrap_or(0);
                let hex_str = row[1].as_str().unwrap_or("");
                let samples = hex_to_samples(hex_str);
                results.push((ts, samples));
            }
        }

        Ok(results)
    }
}

/// hex 字符串转换为采样值数组
fn hex_to_samples(hex_str: &str) -> Vec<i16> {
    let mut samples = Vec::new();
    let chars: Vec<char> = hex_str.chars().collect();
    let mut i = 0;
    while i + 4 <= chars.len() {
        let hex: String = chars[i..i + 4].iter().collect();
        if let Ok(val) = u16::from_str_radix(&hex, 16) {
            samples.push(val as i16);
        }
        i += 4;
    }
    samples
}
