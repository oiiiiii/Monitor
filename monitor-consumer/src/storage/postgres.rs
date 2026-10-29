//! PostgreSQL 关系型数据库存储
//!
//! 负责生命体征、报警事件等结构化数据的持久化。
//! 遵循项目规范：
//! - 生命体征表、报警表按时间分区
//! - 建立床位+时间联合索引，保障查询性能
//! - 设备配置、阈值等基础数据独立表存储

use std::time::Duration;

use deadpool_postgres::{Config as PoolConfig, Runtime, Timeouts};
use tokio_postgres::{self, NoTls, Row};
use tracing::{error, info, warn};

use crate::config::PostgresConfig;
use monitor_common::types::{AlarmEvent, VitalSignsData};

/// PostgreSQL 存储
pub struct PostgresStore {
    pool: deadpool_postgres::Pool,
}

impl PostgresStore {
    /// 创建新的 PostgreSQL 存储
    pub fn new(config: &PostgresConfig) -> anyhow::Result<Self> {
        let mut pool_config = PoolConfig::new();
        pool_config.url = Some(config.database_url());
        pool_config.pool = Some(deadpool_postgres::PoolConfig {
            max_size: config.pool_size,
            timeouts: Timeouts {
                wait: Some(Duration::from_secs(5)),
                create: Some(Duration::from_secs(5)),
                recycle: Some(Duration::from_secs(5)),
            },
            ..Default::default()
        });

        let pool = pool_config.create_pool(Some(Runtime::Tokio1), NoTls)?;

        Ok(Self { pool })
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &deadpool_postgres::Pool {
        &self.pool
    }

    /// 初始化（创建表结构）
    pub async fn init(&self) -> anyhow::Result<()> {
        info!("初始化 PostgreSQL 数据库");

        let client = self.pool.get().await?;

        // 1. 生命体征表（按月分区，这里先创建主表）
        let create_vital_sql = r#"
            CREATE TABLE IF NOT EXISTS vital_signs (
                id BIGSERIAL PRIMARY KEY,
                ts TIMESTAMPTZ NOT NULL,
                bed_no INTEGER NOT NULL,
                device_ip VARCHAR(50) NOT NULL,
                vendor VARCHAR(20) NOT NULL,
                model VARCHAR(50),
                serial VARCHAR(50),
                heart_rate INTEGER,
                pulse_rate INTEGER,
                spo2 INTEGER,
                resp_rate INTEGER,
                nibp_systolic INTEGER,
                nibp_diastolic INTEGER,
                nibp_mean INTEGER,
                temperature REAL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_vital_signs_bed_ts
            ON vital_signs (bed_no, ts DESC);

            CREATE INDEX IF NOT EXISTS idx_vital_signs_ts
            ON vital_signs (ts DESC);
        "#;
        client.batch_execute(create_vital_sql).await?;
        info!("生命体征表创建完成");

        // 2. 报警事件表
        let create_alarm_sql = r#"
            CREATE TABLE IF NOT EXISTS alarm_events (
                id BIGSERIAL PRIMARY KEY,
                ts TIMESTAMPTZ NOT NULL,
                bed_no INTEGER NOT NULL,
                device_ip VARCHAR(50) NOT NULL,
                vendor VARCHAR(20) NOT NULL,
                alarm_code VARCHAR(100) NOT NULL,
                alarm_name VARCHAR(200),
                priority VARCHAR(10) NOT NULL,
                event_phase VARCHAR(20) NOT NULL,
                trigger_param VARCHAR(100),
                trigger_value REAL,
                low_limit REAL,
                high_limit REAL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_alarm_events_bed_ts
            ON alarm_events (bed_no, ts DESC);

            CREATE INDEX IF NOT EXISTS idx_alarm_events_ts
            ON alarm_events (ts DESC);

            CREATE INDEX IF NOT EXISTS idx_alarm_events_priority
            ON alarm_events (priority);
        "#;
        client.batch_execute(create_alarm_sql).await?;
        info!("报警事件表创建完成");

        // 3. 设备信息表
        let create_device_sql = r#"
            CREATE TABLE IF NOT EXISTS devices (
                id SERIAL PRIMARY KEY,
                device_ip VARCHAR(50) UNIQUE NOT NULL,
                bed_no INTEGER,
                vendor VARCHAR(20),
                model VARCHAR(50),
                serial VARCHAR(50),
                first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                is_online BOOLEAN NOT NULL DEFAULT TRUE
            );

            CREATE INDEX IF NOT EXISTS idx_devices_bed_no
            ON devices (bed_no);

            CREATE INDEX IF NOT EXISTS idx_devices_ip
            ON devices (device_ip);
        "#;
        client.batch_execute(create_device_sql).await?;
        info!("设备信息表创建完成");

        Ok(())
    }

    /// 插入一条生命体征数据
    pub async fn insert_vital_signs(
        &self,
        vital: &VitalSignsData,
    ) -> anyhow::Result<i64> {
        let client = self.pool.get().await?;

        let stmt = r#"
            INSERT INTO vital_signs (
                ts, bed_no, device_ip, vendor, model, serial,
                heart_rate, spo2, resp_rate,
                nibp_sys, nibp_dia, nibp_map
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9,
                $10, $11, $12
            ) RETURNING id;
        "#;

        let row = client
            .query_one(
                stmt,
                &[
                    &vital.sample_time,
                    &(vital.device.bed_no as i32),
                    &vital.device.device_ip,
                    &vital.device.vendor.to_string(),
                    &vital.device.model,
                    &vital.device.serial,
                    &vital.heart_rate.map(|v| v as i32),
                    &vital.spo2.map(|v| v as i32),
                    &vital.resp_rate.map(|v| v as i32),
                    &vital.nibp_sys.map(|v| v as i32),
                    &vital.nibp_dia.map(|v| v as i32),
                    &vital.nibp_map.map(|v| v as i32),
                ],
            )
            .await?;

        let id: i64 = row.get(0);

        // 更新设备信息
        self.upsert_device(&vital.device.device_ip, vital.device.bed_no as i32,
            &vital.device.vendor.to_string(),
            &vital.device.model,
            &vital.device.serial
        ).await?;

        Ok(id)
    }

    /// 插入一条报警事件
    pub async fn insert_alarm(
        &self,
        alarm: &AlarmEvent,
    ) -> anyhow::Result<i64> {
        let client = self.pool.get().await?;

        let stmt = r#"
            INSERT INTO alarm_events (
                ts, bed_no, device_ip, vendor,
                alarm_code, alarm_name, priority, event_phase,
                trigger_param, trigger_value, threshold
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6, $7, $8,
                $9, $10, $11
            ) RETURNING id;
        "#;

        let row = client
            .query_one(
                stmt,
                &[
                    &alarm.alarm_time,
                    &(alarm.device.bed_no as i32),
                    &alarm.device.device_ip,
                    &alarm.device.vendor.to_string(),
                    &alarm.alarm_code,
                    &alarm.alarm_name,
                    &alarm.priority.to_string(),
                    &alarm.event_phase,
                    &alarm.trigger_param,
                    &alarm.trigger_value,
                    &alarm.threshold,
                ],
            )
            .await?;

        let id: i64 = row.get(0);
        Ok(id)
    }

    /// 更新或插入设备信息
    pub async fn upsert_device(
        &self,
        device_ip: &str,
        bed_no: i32,
        vendor: &str,
        model: &str,
        serial: &str,
    ) -> anyhow::Result<()> {
        let client = self.pool.get().await?;

        let stmt = r#"
            INSERT INTO devices (device_ip, bed_no, vendor, model, serial, last_seen, is_online)
            VALUES ($1, $2, $3, $4, $5, NOW(), TRUE)
            ON CONFLICT (device_ip) DO UPDATE
            SET bed_no = EXCLUDED.bed_no,
                vendor = EXCLUDED.vendor,
                model = EXCLUDED.model,
                serial = EXCLUDED.serial,
                last_seen = NOW(),
                is_online = TRUE;
        "#;

        client.execute(stmt, &[&device_ip, &bed_no, &vendor, &model, &serial]).await?;

        Ok(())
    }

    /// 标记设备离线
    pub async fn mark_device_offline(&self, device_ip: &str) -> anyhow::Result<()> {
        let client = self.pool.get().await?;

        let stmt = "UPDATE devices SET is_online = FALSE WHERE device_ip = $1;";
        client.execute(stmt, &[&device_ip]).await?;

        Ok(())
    }

    /// 查询指定床位的最近 N 条生命体征
    pub async fn query_recent_vitals(
        &self,
        bed_no: u16,
        limit: i64,
    ) -> anyhow::Result<Vec<VitalSignsData>> {
        // 这里简化实现，实际项目中需要完整的字段映射
        Ok(Vec::new())
    }
}
