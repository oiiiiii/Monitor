//! SQLite 存储引擎（体征 + 报警 + 设备信息）

use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Duration, Timelike, Utc};
use monitor_common::types::{DeviceInfo, VitalSignsData};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tracing::{debug, info, warn};

/// 体征数据记录（用于API返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VitalSignRecord {
    pub ts: String,
    pub bed_no: u16,
    pub heart_rate: Option<i32>,
    pub spo2: Option<i32>,
    pub resp_rate: Option<i32>,
    pub nibp_sys: Option<i32>,
    pub nibp_dia: Option<i32>,
    pub nibp_map: Option<i32>,
    pub ibp_sys: Option<i32>,
    pub ibp_dia: Option<i32>,
    pub ibp_map: Option<i32>,
    pub pulse_rate: Option<i32>,
    pub pi: Option<f64>,
}

/// SQLite 存储
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// 打开/创建数据库
    pub async fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_lazy(&format!("sqlite:{}?mode=rwc", db_path))?;

        let storage = Self { pool };
        storage.init_schema().await?;

        sqlx::query("PRAGMA journal_mode = WAL").execute(&storage.pool).await?;
        sqlx::query("PRAGMA synchronous = NORMAL").execute(&storage.pool).await?;
        sqlx::query("PRAGMA cache_size = 10000").execute(&storage.pool).await?;

        info!("SQLite 数据库已就绪: {}", db_path);
        Ok(storage)
    }

    /// 初始化表结构
    async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bed_no INTEGER NOT NULL UNIQUE,
                device_ip TEXT,
                vendor TEXT,
                model TEXT,
                serial TEXT,
                last_seen TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS vital_signs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                bed_no INTEGER NOT NULL,
                heart_rate INTEGER,
                spo2 INTEGER,
                resp_rate INTEGER,
                nibp_sys INTEGER,
                nibp_dia INTEGER,
                nibp_map INTEGER,
                ibp_sys INTEGER,
                ibp_dia INTEGER,
                ibp_map INTEGER
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // 为已存在的表添加 IBP 字段（如果不存在）
        sqlx::query("ALTER TABLE vital_signs ADD COLUMN ibp_sys INTEGER")
            .execute(&self.pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE vital_signs ADD COLUMN ibp_dia INTEGER")
            .execute(&self.pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE vital_signs ADD COLUMN ibp_map INTEGER")
            .execute(&self.pool)
            .await
            .ok();

        sqlx::query("ALTER TABLE vital_signs ADD COLUMN pulse_rate INTEGER")
            .execute(&self.pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE vital_signs ADD COLUMN pi REAL")
            .execute(&self.pool)
            .await
            .ok();

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_vital_bed_ts ON vital_signs (bed_no, ts DESC)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS alarm_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                bed_no INTEGER NOT NULL,
                alarm_code TEXT NOT NULL,
                alarm_name TEXT,
                priority TEXT,
                event_phase TEXT,
                trigger_param TEXT,
                trigger_value REAL,
                threshold REAL,
                acknowledged INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_alarm_bed_ts ON alarm_events (bed_no, ts DESC)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_devices_bed_no ON devices (bed_no)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 插入/更新设备信息
    pub async fn upsert_device(&self, device: &DeviceInfo) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO devices (bed_no, device_ip, vendor, model, serial, last_seen, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(bed_no) DO UPDATE SET
                device_ip = excluded.device_ip,
                vendor = excluded.vendor,
                model = excluded.model,
                serial = excluded.serial,
                last_seen = excluded.last_seen
            "#,
        )
        .bind(device.bed_no as i32)
        .bind(&device.device_ip)
        .bind(device.vendor.to_string())
        .bind(&device.model)
        .bind(&device.serial)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        debug!(bed_no = device.bed_no, "设备信息已更新");
        Ok(())
    }

    /// 插入生命体征数据（5分钟采样策略：心率/呼吸/血氧/IBP每5分钟一条，NIBP每条都存储）
    pub async fn insert_vital(&self, vital: &VitalSignsData) -> Result<i64> {
        const SAMPLE_INTERVAL_MINUTES: i64 = 5;
        let bed_no = vital.device.bed_no as i32;

        let has_nibp = vital.nibp_sys.is_some() || vital.nibp_dia.is_some() || vital.nibp_map.is_some();

        if has_nibp {
            let ts = vital.sample_time.to_rfc3339();
            debug!(bed_no = vital.device.bed_no, "NIBP数据，直接插入新记录");
            let result = sqlx::query(
                r#"
                INSERT INTO vital_signs (
                    ts, bed_no,
                    heart_rate, spo2, resp_rate,
                    nibp_sys, nibp_dia, nibp_map,
                    ibp_sys, ibp_dia, ibp_map,
                    pulse_rate, pi
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&ts)
            .bind(bed_no)
            .bind(vital.heart_rate.map(|v| v as i32))
            .bind(vital.spo2.map(|v| v as i32))
            .bind(vital.resp_rate.map(|v| v as i32))
            .bind(vital.nibp_sys.map(|v| v as i32))
            .bind(vital.nibp_dia.map(|v| v as i32))
            .bind(vital.nibp_map.map(|v| v as i32))
            .bind(vital.ibp_sys.map(|v| v as i32))
            .bind(vital.ibp_dia.map(|v| v as i32))
            .bind(vital.ibp_map.map(|v| v as i32))
            .bind(vital.pulse_rate.map(|v| v as i32))
            .bind(vital.pi)
            .execute(&self.pool)
            .await?;

            self.upsert_device(&vital.device).await?;
            return Ok(result.last_insert_rowid());
        }

        let bucket_start = vital.sample_time
            .with_minute((vital.sample_time.minute() / SAMPLE_INTERVAL_MINUTES as u32) * SAMPLE_INTERVAL_MINUTES as u32)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();
        let bucket_ts = bucket_start.to_rfc3339();

        let existing_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM vital_signs WHERE bed_no = ? AND ts = ?"
        )
        .bind(bed_no)
        .bind(&bucket_ts)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing_id) = existing_id {
            debug!(bed_no = vital.device.bed_no, "5分钟区间已有记录，更新快速指标");
            let result = sqlx::query(
                r#"
                UPDATE vital_signs SET
                    heart_rate = COALESCE(?, heart_rate),
                    spo2 = COALESCE(?, spo2),
                    resp_rate = COALESCE(?, resp_rate),
                    ibp_sys = COALESCE(?, ibp_sys),
                    ibp_dia = COALESCE(?, ibp_dia),
                    ibp_map = COALESCE(?, ibp_map),
                    pulse_rate = COALESCE(?, pulse_rate),
                    pi = COALESCE(?, pi)
                WHERE id = ?
                "#,
            )
            .bind(vital.heart_rate.map(|v| v as i32))
            .bind(vital.spo2.map(|v| v as i32))
            .bind(vital.resp_rate.map(|v| v as i32))
            .bind(vital.ibp_sys.map(|v| v as i32))
            .bind(vital.ibp_dia.map(|v| v as i32))
            .bind(vital.ibp_map.map(|v| v as i32))
            .bind(vital.pulse_rate.map(|v| v as i32))
            .bind(vital.pi)
            .bind(existing_id)
            .execute(&self.pool)
            .await?;

            self.upsert_device(&vital.device).await?;
            return Ok(result.last_insert_rowid());
        }

        debug!(bed_no = vital.device.bed_no, "5分钟区间无记录，插入新记录");
        let result = sqlx::query(
            r#"
            INSERT INTO vital_signs (
                ts, bed_no,
                heart_rate, spo2, resp_rate,
                nibp_sys, nibp_dia, nibp_map,
                ibp_sys, ibp_dia, ibp_map,
                pulse_rate, pi
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&bucket_ts)
        .bind(bed_no)
        .bind(vital.heart_rate.map(|v| v as i32))
        .bind(vital.spo2.map(|v| v as i32))
        .bind(vital.resp_rate.map(|v| v as i32))
        .bind(vital.nibp_sys.map(|v| v as i32))
        .bind(vital.nibp_dia.map(|v| v as i32))
        .bind(vital.nibp_map.map(|v| v as i32))
        .bind(vital.ibp_sys.map(|v| v as i32))
        .bind(vital.ibp_dia.map(|v| v as i32))
        .bind(vital.ibp_map.map(|v| v as i32))
        .bind(vital.pulse_rate.map(|v| v as i32))
        .bind(vital.pi)
        .execute(&self.pool)
        .await?;

        self.upsert_device(&vital.device).await?;

        Ok(result.last_insert_rowid())
    }

    /// 查询历史体征数据
    pub async fn get_vitals(
        &self,
        bed_no: u16,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<VitalSignRecord>> {
        let mut query = String::from(
            "SELECT ts, bed_no, heart_rate, spo2, resp_rate, nibp_sys, nibp_dia, nibp_map, ibp_sys, ibp_dia, ibp_map FROM vital_signs WHERE bed_no = ?"
        );
        let mut bindings: Vec<String> = vec![bed_no.to_string()];

        if let Some(start_time) = start {
            query.push_str(" AND ts >= ?");
            bindings.push(start_time.to_rfc3339());
        }
        if let Some(end_time) = end {
            query.push_str(" AND ts <= ?");
            bindings.push(end_time.to_rfc3339());
        }

        query.push_str(" ORDER BY ts DESC LIMIT ?");
        bindings.push(limit.to_string());

        let mut q = sqlx::query_as::<_, (
            String,
            i32,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<f64>,
        )>(&query);

        q = q.bind(bed_no as i32);
        if let Some(start_time) = start {
            q = q.bind(start_time.to_rfc3339());
        }
        if let Some(end_time) = end {
            q = q.bind(end_time.to_rfc3339());
        }
        q = q.bind(limit as i64);

        let rows = q.fetch_all(&self.pool).await?;

        let records: Vec<VitalSignRecord> = rows
            .into_iter()
            .map(|(ts_str, _bed_no, hr, spo2, resp, nibp_sys, nibp_dia, nibp_map, ibp_sys, ibp_dia, ibp_map, pulse_rate, pi)| {
                VitalSignRecord {
                    ts: ts_str,
                    bed_no: _bed_no as u16,
                    heart_rate: hr,
                    spo2,
                    resp_rate: resp,
                    nibp_sys,
                    nibp_dia,
                    nibp_map,
                    ibp_sys,
                    ibp_dia,
                    ibp_map,
                    pulse_rate,
                    pi,
                }
            })
            .collect();

        Ok(records)
    }

    /// 按时间间隔采样查询体征数据（在数据库层面完成采样，提升性能）
    pub async fn get_vitals_at_interval(
        &self,
        bed_no: u16,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        interval_minutes: u32,
    ) -> Result<Vec<VitalSignRecord>> {
        let start_time = start.unwrap_or_else(|| Utc::now() - Duration::hours(24));
        let end_time = end.unwrap_or_else(Utc::now);

        let mut query = String::from(
            "SELECT ts, bed_no, heart_rate, spo2, resp_rate, nibp_sys, nibp_dia, nibp_map, ibp_sys, ibp_dia, ibp_map, pulse_rate, pi FROM vital_signs WHERE bed_no = ?"
        );

        query.push_str(" AND ts >= ?");
        query.push_str(" AND ts <= ?");
        query.push_str(" ORDER BY ts ASC");

        let mut q = sqlx::query_as::<_, (
            String,
            i32,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<f64>,
        )>(&query);

        q = q.bind(bed_no as i32);
        q = q.bind(start_time.to_rfc3339());
        q = q.bind(end_time.to_rfc3339());

        let rows = q.fetch_all(&self.pool).await?;

        let mut records: Vec<VitalSignRecord> = rows
            .into_iter()
            .map(|(ts_str, _bed_no, hr, spo2, resp, nibp_sys, nibp_dia, nibp_map, ibp_sys, ibp_dia, ibp_map, pulse_rate, pi)| {
                VitalSignRecord {
                    ts: ts_str,
                    bed_no: _bed_no as u16,
                    heart_rate: hr,
                    spo2,
                    resp_rate: resp,
                    nibp_sys,
                    nibp_dia,
                    nibp_map,
                    ibp_sys,
                    ibp_dia,
                    ibp_map,
                    pulse_rate,
                    pi,
                }
            })
            .collect();

        records = Self::aggregate_by_5min_interval_with_filled_buckets(records, start_time, end_time, bed_no);

        if interval_minutes == 30 {
            records = Self::merge_intervals(records, 6);
        } else if interval_minutes == 60 {
            records = Self::merge_intervals(records, 12);
        }

        Ok(records)
    }

    fn aggregate_by_5min_interval_with_filled_buckets(
        records: Vec<VitalSignRecord>,
        _start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        bed_no: u16,
    ) -> Vec<VitalSignRecord> {
        let aggregated = Self::aggregate_by_5min_interval(records);

        if aggregated.is_empty() {
            return aggregated;
        }

        let mut result: Vec<VitalSignRecord> = Vec::new();
        
        let now = Utc::now();
        let current_bucket = now
            .with_minute((now.minute() / 5) * 5)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        let mut current_time = match DateTime::parse_from_rfc3339(&aggregated[0].ts) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => Utc::now(),
        };

        let mut record_idx = 0;

        while current_time <= current_bucket {
            let bucket_key = current_time.to_rfc3339();
            
            let found_record = if record_idx < aggregated.len() {
                let record = &aggregated[record_idx];
                if record.ts == bucket_key {
                    record_idx += 1;
                    Some(record.clone())
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(record) = found_record {
                result.push(record);
            } else {
                result.push(VitalSignRecord {
                    ts: bucket_key,
                    bed_no,
                    heart_rate: None,
                    spo2: None,
                    resp_rate: None,
                    nibp_sys: None,
                    nibp_dia: None,
                    nibp_map: None,
                    ibp_sys: None,
                    ibp_dia: None,
                    ibp_map: None,
                    pulse_rate: None,
                    pi: None,
                });
            }

            current_time += Duration::minutes(5);
        }

        result
    }

    fn aggregate_by_5min_interval(mut records: Vec<VitalSignRecord>) -> Vec<VitalSignRecord> {
        let mut result: Vec<VitalSignRecord> = Vec::new();
        let mut current_bucket: Option<String> = None;
        let mut current_record: Option<VitalSignRecord> = None;

        for record in records {
            let ts = match DateTime::parse_from_rfc3339(&record.ts) {
                Ok(t) => t.with_timezone(&Utc),
                Err(_) => continue,
            };

            let bucket_minute = (ts.minute() / 5) * 5;
            let bucket_start = ts
                .with_minute(bucket_minute)
                .unwrap()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap();
            let bucket_key = bucket_start.to_rfc3339();

            if current_bucket.as_deref() != Some(&bucket_key) {
                if let Some(r) = current_record.take() {
                    result.push(r);
                }
                current_bucket = Some(bucket_key.clone());
                current_record = Some(VitalSignRecord {
                    ts: bucket_key,
                    ..record
                });
            } else if let Some(r) = current_record.as_mut() {
                if r.heart_rate.is_none() && record.heart_rate.is_some() {
                    r.heart_rate = record.heart_rate;
                }
                if r.spo2.is_none() && record.spo2.is_some() {
                    r.spo2 = record.spo2;
                }
                if r.resp_rate.is_none() && record.resp_rate.is_some() {
                    r.resp_rate = record.resp_rate;
                }
                if r.ibp_sys.is_none() && record.ibp_sys.is_some() {
                    r.ibp_sys = record.ibp_sys;
                }
                if r.ibp_dia.is_none() && record.ibp_dia.is_some() {
                    r.ibp_dia = record.ibp_dia;
                }
                if r.ibp_map.is_none() && record.ibp_map.is_some() {
                    r.ibp_map = record.ibp_map;
                }
                if r.nibp_sys.is_none() && record.nibp_sys.is_some() {
                    r.nibp_sys = record.nibp_sys;
                }
                if r.nibp_dia.is_none() && record.nibp_dia.is_some() {
                    r.nibp_dia = record.nibp_dia;
                }
                if r.nibp_map.is_none() && record.nibp_map.is_some() {
                    r.nibp_map = record.nibp_map;
                }
                if r.pulse_rate.is_none() && record.pulse_rate.is_some() {
                    r.pulse_rate = record.pulse_rate;
                }
                if r.pi.is_none() && record.pi.is_some() {
                    r.pi = record.pi;
                }
            }
        }

        if let Some(r) = current_record {
            result.push(r);
        }

        result
    }

    fn merge_intervals(records: Vec<VitalSignRecord>, merge_count: usize) -> Vec<VitalSignRecord> {
        let mut result: Vec<VitalSignRecord> = Vec::new();
        let mut merged_record: Option<VitalSignRecord> = None;

        for (i, record) in records.iter().enumerate() {
            if i % merge_count == 0 {
                if let Some(r) = merged_record.take() {
                    result.push(r);
                }
                merged_record = Some(record.clone());
            } else if let Some(r) = merged_record.as_mut() {
                if r.heart_rate.is_none() && record.heart_rate.is_some() {
                    r.heart_rate = record.heart_rate;
                }
                if r.spo2.is_none() && record.spo2.is_some() {
                    r.spo2 = record.spo2;
                }
                if r.resp_rate.is_none() && record.resp_rate.is_some() {
                    r.resp_rate = record.resp_rate;
                }
                if r.ibp_sys.is_none() && record.ibp_sys.is_some() {
                    r.ibp_sys = record.ibp_sys;
                }
                if r.ibp_dia.is_none() && record.ibp_dia.is_some() {
                    r.ibp_dia = record.ibp_dia;
                }
                if r.ibp_map.is_none() && record.ibp_map.is_some() {
                    r.ibp_map = record.ibp_map;
                }
                if r.nibp_sys.is_none() && record.nibp_sys.is_some() {
                    r.nibp_sys = record.nibp_sys;
                }
                if r.nibp_dia.is_none() && record.nibp_dia.is_some() {
                    r.nibp_dia = record.nibp_dia;
                }
                if r.nibp_map.is_none() && record.nibp_map.is_some() {
                    r.nibp_map = record.nibp_map;
                }
                if r.pulse_rate.is_none() && record.pulse_rate.is_some() {
                    r.pulse_rate = record.pulse_rate;
                }
                if r.pi.is_none() && record.pi.is_some() {
                    r.pi = record.pi;
                }
            }
        }

        if let Some(r) = merged_record {
            result.push(r);
        }

        result
    }

    /// 清理过期数据（仅清理体征数据）
    pub async fn cleanup_old_data(
        &self,
        vital_hours: u32,
    ) -> Result<u64> {
        let vital_cutoff = Utc::now() - Duration::hours(vital_hours as i64);

        let vital_result = sqlx::query(
            r#"DELETE FROM vital_signs WHERE ts < ?"#,
        )
        .bind(vital_cutoff.to_rfc3339())
        .execute(&self.pool)
        .await?;

        let vital_deleted = vital_result.rows_affected();

        if vital_deleted > 0 {
            warn!(
                vital_deleted,
                "已清理过期体征数据"
            );
        }

        Ok(vital_deleted)
    }
}
