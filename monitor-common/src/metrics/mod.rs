//! 监控指标模块
//!
//! 提供系统运行时指标采集：
//! - 计数器（Counter）：消息数、错误数、字节数
//! - 仪表盘（Gauge）：连接数、队列长度、在线设备数
//! - 统计：最近 1 分钟/5 分钟/15 分钟速率
//!
//! 使用原子变量实现，无锁，高性能。

pub mod health_server;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

/// 计数器（单调递增）
#[derive(Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// 递增 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加 N
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    /// 获取当前值
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// 重置并返回旧值
    pub fn reset(&self) -> u64 {
        self.value.swap(0, Ordering::Relaxed)
    }
}

/// 仪表盘（可升可降）
#[derive(Default)]
pub struct Gauge {
    value: AtomicU64,
}

impl Gauge {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// 设置值
    pub fn set(&self, v: u64) {
        self.value.store(v, Ordering::Relaxed);
    }

    /// 加 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 减 1
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    /// 获取值
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// 速率计算器（滑动窗口）
pub struct RateMeter {
    counter: Counter,
    start_time: Instant,
    last_value: u64,
    last_time: Instant,
}

impl RateMeter {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            counter: Counter::new(),
            start_time: now,
            last_value: 0,
            last_time: now,
        }
    }

    /// 标记一次事件
    pub fn mark(&self) {
        self.counter.inc();
    }

    /// 标记 N 次
    pub fn mark_n(&self, n: u64) {
        self.counter.add(n);
    }

    /// 获取总计数
    pub fn count(&self) -> u64 {
        self.counter.get()
    }

    /// 获取平均速率（每秒）
    pub fn mean_rate(&self) -> f64 {
        let count = self.counter.get() as f64;
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed == 0.0 {
            0.0
        } else {
            count / elapsed
        }
    }
}

impl Default for RateMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// 接入层指标
pub struct AccessMetrics {
    /// 已接受的连接数（累计）
    pub connections_total: Counter,
    /// 当前活跃连接数
    pub connections_active: Gauge,
    /// 接收的 HL7 帧数
    pub frames_received: Counter,
    /// 接收的字节数
    pub bytes_received: Counter,
    /// 解析成功的消息数
    pub messages_parsed: Counter,
    /// 解析失败数
    pub parse_errors: Counter,
    /// 发送到 MQ 的消息数
    pub mq_sent: Counter,
    /// MQ 发送失败数
    pub mq_send_errors: Counter,
    /// 磁盘缓冲写入数
    pub disk_buffer_writes: Counter,
    /// 启动时间
    pub start_time: Instant,
}

impl AccessMetrics {
    pub fn new() -> Self {
        Self {
            connections_total: Counter::new(),
            connections_active: Gauge::new(),
            frames_received: Counter::new(),
            bytes_received: Counter::new(),
            messages_parsed: Counter::new(),
            parse_errors: Counter::new(),
            mq_sent: Counter::new(),
            mq_send_errors: Counter::new(),
            disk_buffer_writes: Counter::new(),
            start_time: Instant::now(),
        }
    }

    /// 生成快照（用于健康检查接口）
    pub fn snapshot(&self) -> AccessMetricsSnapshot {
        AccessMetricsSnapshot {
            connections_total: self.connections_total.get(),
            connections_active: self.connections_active.get(),
            frames_received: self.frames_received.get(),
            bytes_received: self.bytes_received.get(),
            messages_parsed: self.messages_parsed.get(),
            parse_errors: self.parse_errors.get(),
            mq_sent: self.mq_sent.get(),
            mq_send_errors: self.mq_send_errors.get(),
            disk_buffer_writes: self.disk_buffer_writes.get(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }
}

impl Default for AccessMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 接入层指标快照
#[derive(Debug, Serialize)]
pub struct AccessMetricsSnapshot {
    pub connections_total: u64,
    pub connections_active: u64,
    pub frames_received: u64,
    pub bytes_received: u64,
    pub messages_parsed: u64,
    pub parse_errors: u64,
    pub mq_sent: u64,
    pub mq_send_errors: u64,
    pub disk_buffer_writes: u64,
    pub uptime_secs: u64,
}

/// 消费层指标
pub struct ConsumerMetrics {
    /// 消费的消息总数
    pub messages_consumed: Counter,
    /// 写入 TDengine 的波形点数
    pub tdengine_wave_points: Counter,
    /// 写入 PostgreSQL 的体征数
    pub pg_vital_writes: Counter,
    /// 写入 PostgreSQL 的报警数
    pub pg_alarm_writes: Counter,
    /// 消费失败重试次数
    pub retry_count: Counter,
    /// 死信队列消息数
    pub dead_letter_count: Counter,
    /// 当前堆积的消息数（估算）
    pub queue_backlog: Gauge,
    /// 启动时间
    pub start_time: Instant,
}

impl ConsumerMetrics {
    pub fn new() -> Self {
        Self {
            messages_consumed: Counter::new(),
            tdengine_wave_points: Counter::new(),
            pg_vital_writes: Counter::new(),
            pg_alarm_writes: Counter::new(),
            retry_count: Counter::new(),
            dead_letter_count: Counter::new(),
            queue_backlog: Gauge::new(),
            start_time: Instant::now(),
        }
    }

    pub fn snapshot(&self) -> ConsumerMetricsSnapshot {
        ConsumerMetricsSnapshot {
            messages_consumed: self.messages_consumed.get(),
            tdengine_wave_points: self.tdengine_wave_points.get(),
            pg_vital_writes: self.pg_vital_writes.get(),
            pg_alarm_writes: self.pg_alarm_writes.get(),
            retry_count: self.retry_count.get(),
            dead_letter_count: self.dead_letter_count.get(),
            queue_backlog: self.queue_backlog.get(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }
}

impl Default for ConsumerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
pub struct ConsumerMetricsSnapshot {
    pub messages_consumed: u64,
    pub tdengine_wave_points: u64,
    pub pg_vital_writes: u64,
    pub pg_alarm_writes: u64,
    pub retry_count: u64,
    pub dead_letter_count: u64,
    pub queue_backlog: u64,
    pub uptime_secs: u64,
}

/// 推送层指标
pub struct PushMetrics {
    /// WebSocket 连接总数（累计）
    pub ws_connections_total: Counter,
    /// 当前活跃 WebSocket 连接数
    pub ws_connections_active: Gauge,
    /// 推送的体征消息数
    pub vital_pushed: Counter,
    /// 推送的波形消息数
    pub wave_pushed: Counter,
    /// 推送的报警消息数
    pub alarm_pushed: Counter,
    /// 订阅的床位数（累计订阅次数）
    pub subscriptions_total: Counter,
    /// Redis 缓存命中数
    pub cache_hits: Counter,
    /// Redis 缓存未命中数
    pub cache_misses: Counter,
    /// 启动时间
    pub start_time: Instant,
}

impl PushMetrics {
    pub fn new() -> Self {
        Self {
            ws_connections_total: Counter::new(),
            ws_connections_active: Gauge::new(),
            vital_pushed: Counter::new(),
            wave_pushed: Counter::new(),
            alarm_pushed: Counter::new(),
            subscriptions_total: Counter::new(),
            cache_hits: Counter::new(),
            cache_misses: Counter::new(),
            start_time: Instant::now(),
        }
    }

    pub fn snapshot(&self) -> PushMetricsSnapshot {
        PushMetricsSnapshot {
            ws_connections_total: self.ws_connections_total.get(),
            ws_connections_active: self.ws_connections_active.get(),
            vital_pushed: self.vital_pushed.get(),
            wave_pushed: self.wave_pushed.get(),
            alarm_pushed: self.alarm_pushed.get(),
            subscriptions_total: self.subscriptions_total.get(),
            cache_hits: self.cache_hits.get(),
            cache_misses: self.cache_misses.get(),
            cache_hit_rate: if self.cache_hits.get() + self.cache_misses.get() > 0 {
                self.cache_hits.get() as f64 / (self.cache_hits.get() + self.cache_misses.get()) as f64
            } else {
                0.0
            },
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }
}

impl Default for PushMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
pub struct PushMetricsSnapshot {
    pub ws_connections_total: u64,
    pub ws_connections_active: u64,
    pub vital_pushed: u64,
    pub wave_pushed: u64,
    pub alarm_pushed: u64,
    pub subscriptions_total: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub uptime_secs: u64,
}

/// 系统健康状态
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// 完全正常
    Healthy,
    /// 降级运行（部分功能可用）
    Degraded,
    /// 不可用
    Unhealthy,
}

/// 健康检查结果
#[derive(Debug, Serialize)]
pub struct HealthCheck {
    pub status: HealthStatus,
    pub service: String,
    pub version: String,
    pub uptime_secs: u64,
    pub components: Vec<ComponentHealth>,
}

/// 组件健康状态
#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
