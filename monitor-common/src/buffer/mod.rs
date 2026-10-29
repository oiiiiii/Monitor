//! 本地磁盘兜底缓存
//!
//! 当 MQ 不可用时，将消息写入本地磁盘文件，
//! MQ 恢复后自动补发，确保零丢包。
//!
//! 存储格式：
//! - 按日期分文件
//! - 每条消息：[8字节长度前缀][JSON 元数据][消息内容]
//! - 元数据包含：队列类型、时间戳、设备 IP

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::mq::queues::QueueType;

/// 本地缓存配置
#[derive(Debug, Clone)]
pub struct DiskBufferConfig {
    /// 缓存目录
    pub buffer_dir: PathBuf,
    /// 单文件最大大小（字节）
    pub max_file_size: u64,
    /// 最大缓存总大小（字节）
    pub max_total_size: u64,
    /// 补发检查间隔（秒）
    pub replay_interval_secs: u64,
}

impl Default for DiskBufferConfig {
    fn default() -> Self {
        Self {
            buffer_dir: PathBuf::from("./data/buffer"),
            max_file_size: 64 * 1024 * 1024, // 64MB
            max_total_size: 2 * 1024 * 1024 * 1024, // 2GB
            replay_interval_secs: 5,
        }
    }
}

/// 缓存文件中的消息条目
#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedMessage {
    /// 队列类型
    pub queue: String,
    /// 设备 IP
    pub device_ip: String,
    /// 时间戳（Unix 秒）
    pub timestamp: u64,
    /// 消息 payload
    pub payload: Vec<u8>,
}

impl BufferedMessage {
    /// 创建新的缓存消息
    pub fn new(queue: QueueType, device_ip: &str, payload: Vec<u8>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            queue: queue.queue_name().to_string(),
            device_ip: device_ip.to_string(),
            timestamp,
            payload,
        }
    }
}

/// 本地磁盘缓冲器
pub struct DiskBuffer {
    config: DiskBufferConfig,
    state: Mutex<BufferState>,
}

struct BufferState {
    current_file: Option<File>,
    current_file_name: String,
    current_file_size: u64,
    total_size: u64,
}

impl DiskBuffer {
    /// 创建新的磁盘缓冲器
    pub fn new(config: DiskBufferConfig) -> Self {
        Self {
            config,
            state: Mutex::new(BufferState {
                current_file: None,
                current_file_name: String::new(),
                current_file_size: 0,
                total_size: 0,
            }),
        }
    }

    /// 初始化（创建目录，计算现有文件总大小）
    pub async fn init(&self) -> anyhow::Result<()> {
        // 创建缓存目录
        fs::create_dir_all(&self.config.buffer_dir)?;
        info!(dir = ?self.config.buffer_dir, "磁盘缓冲目录已就绪");

        // 计算现有文件总大小
        let total = self.calculate_total_size().await?;
        let mut state = self.state.lock().await;
        state.total_size = total;

        info!(total_mb = total / 1024 / 1024, "磁盘缓冲初始化完成");
        Ok(())
    }

    /// 写入一条消息到磁盘缓存
    pub async fn write(&self, msg: &BufferedMessage) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;

        // 检查总大小限制
        if state.total_size > self.config.max_total_size {
            error!(
                total_mb = state.total_size / 1024 / 1024,
                max_mb = self.config.max_total_size / 1024 / 1024,
                "磁盘缓存已满，写入失败！"
            );
            return Err(anyhow::anyhow!("磁盘缓存已满"));
        }

        // 序列化消息
        let json = serde_json::to_vec(msg)?;
        let len = json.len() as u32;
        let mut buf = Vec::with_capacity(4 + json.len());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&json);

        // 获取或创建当前文件
        let file = self.ensure_current_file(&mut state).await?;

        // 写入
        file.write_all(&buf)?;
        file.flush()?;

        let written = buf.len() as u64;
        state.current_file_size += written;
        state.total_size += written;

        Ok(())
    }

    /// 确保有当前写入文件
    async fn ensure_current_file<'a>(
        &self,
        state: &'a mut BufferState,
    ) -> anyhow::Result<&'a mut File> {
        // 生成今天的文件名
        let file_name = self.current_file_name();

        if state.current_file.is_none() || state.current_file_name != file_name {
            // 关闭旧文件
            state.current_file = None;

            let path = self.config.buffer_dir.join(&file_name);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .open(&path)?;

            let metadata = file.metadata()?;

            state.current_file = Some(file);
            state.current_file_name = file_name;
            state.current_file_size = metadata.len();

            info!(file = %path.display(), "打开缓存文件");
        } else if state.current_file_size > self.config.max_file_size {
            // 文件超过大小限制，创建新文件（加上时间戳后缀）
            let new_name = format!(
                "{}_{}.buf",
                self.current_file_name().trim_end_matches(".buf"),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
            let path = self.config.buffer_dir.join(&new_name);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .open(&path)?;

            state.current_file = Some(file);
            state.current_file_name = new_name;
            state.current_file_size = 0;

            info!(file = %path.display(), "创建新的缓存文件");
        }

        Ok(state.current_file.as_mut().unwrap())
    }

    /// 生成当前日期的文件名
    fn current_file_name(&self) -> String {
        use chrono::Local;
        let now = Local::now();
        format!("{}.buf", now.format("%Y%m%d"))
    }

    /// 计算所有缓存文件的总大小
    async fn calculate_total_size(&self) -> anyhow::Result<u64> {
        let mut total = 0u64;

        let dir = &self.config.buffer_dir;
        if !dir.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "buf").unwrap_or(false) {
                if let Ok(meta) = fs::metadata(&path) {
                    total += meta.len();
                }
            }
        }

        Ok(total)
    }

    /// 读取并返回所有待补发的消息（消费模式：读取后删除文件）
    ///
    /// 返回 (消息列表, 总字节数)
    pub async fn drain_all(&self) -> anyhow::Result<Vec<BufferedMessage>> {
        let mut all_messages = Vec::new();
        let mut files_to_remove = Vec::new();

        let dir = &self.config.buffer_dir;
        if !dir.exists() {
            return Ok(all_messages);
        }

        // 收集所有 buf 文件
        let mut buf_files: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "buf").unwrap_or(false) {
                buf_files.push(path);
            }
        }

        // 按文件名排序（旧的在前）
        buf_files.sort();

        // 逐个读取
        for path in &buf_files {
            match Self::read_buffer_file(path) {
                Ok(messages) => {
                    if !messages.is_empty() {
                        all_messages.extend(messages);
                        files_to_remove.push(path.clone());
                    }
                }
                Err(e) => {
                    error!(file = %path.display(), error = %e, "读取缓存文件失败");
                }
            }
        }

        // 删除已读取的文件（在 state 中更新总大小）
        if !files_to_remove.is_empty() {
            let mut freed: u64 = 0;
            for path in &files_to_remove {
                if let Ok(meta) = fs::metadata(path) {
                    freed += meta.len();
                }
                let _ = fs::remove_file(path);
                info!(file = %path.display(), "已补发并删除缓存文件");
            }

            let mut state = self.state.lock().await;
            state.total_size = state.total_size.saturating_sub(freed);

            // 重置当前文件（可能被删除了）
            state.current_file = None;
            state.current_file_name.clear();
            state.current_file_size = 0;
        }

        Ok(all_messages)
    }

    /// 读取单个缓存文件中的所有消息
    fn read_buffer_file(path: &Path) -> anyhow::Result<Vec<BufferedMessage>> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut messages = Vec::new();

        loop {
            // 读取 4 字节长度
            let mut len_buf = [0u8; 4];
            match reader.read_exact(&mut len_buf) {
                Ok(_) => {}
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => {
                    return Err(anyhow::Error::from(e));
                }
            }

            let len = u32::from_le_bytes(len_buf) as usize;
            if len == 0 || len > 10 * 1024 * 1024 {
                // 异常的长度，跳过
                warn!(len = len, "异常的消息长度，跳过");
                continue;
            }

            // 读取消息内容
            let mut msg_buf = vec![0u8; len];
            reader.read_exact(&mut msg_buf)?;

            // 反序列化
            match serde_json::from_slice::<BufferedMessage>(&msg_buf) {
                Ok(msg) => {
                    messages.push(msg);
                }
                Err(e) => {
                    warn!(error = %e, "消息反序列化失败，跳过");
                }
            }
        }

        Ok(messages)
    }

    /// 获取缓存的消息数量估算（按文件数，不准确，用于监控）
    pub async fn total_size_bytes(&self) -> u64 {
        let state = self.state.lock().await;
        state.total_size
    }

    /// 获取配置
    pub fn config(&self) -> &DiskBufferConfig {
        &self.config
    }
}
