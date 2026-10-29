//! HTTP 服务器（静态文件 + API）

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tracing::{error, info, warn};

use crate::storage::SqliteStorage;

/// HTTP 服务器
pub struct HttpServer {
    listen_addr: String,
    root_dir: PathBuf,
    sqlite: Option<Arc<SqliteStorage>>,
}

impl HttpServer {
    pub fn new(listen_addr: String, root_dir: &str, sqlite: Option<Arc<SqliteStorage>>) -> Self {
        Self {
            listen_addr,
            root_dir: PathBuf::from(root_dir),
            sqlite,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!("HTTP 服务已启动: http://{}", self.listen_addr);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let _ = stream.set_nodelay(true);
                    let root = self.root_dir.clone();
                    let sqlite = self.sqlite.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, root, sqlite).await {
                            warn!("HTTP 连接错误: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("接受连接失败: {}", e);
                }
            }
        }
    }
}

async fn handle_connection(mut stream: TcpStream, root: PathBuf, sqlite: Option<Arc<SqliteStorage>>) -> Result<()> {
    use std::time::Instant;
    let _conn_start = Instant::now();
    
    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();

    let read_line_start = Instant::now();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(())
    }
    tracing::info!(elapsed_ms = read_line_start.elapsed().as_millis(), "HTTP读取请求行耗时");

    let request_line = request_line.trim();
    if !request_line.starts_with("GET ") {
        send_response(&mut stream, 405, "text/plain", "方法不允许".as_bytes()).await?;
        return Ok(());
    }

    let path = {
        let rest = &request_line[4..];
        let end = rest.find(' ').unwrap_or(rest.len());
        rest[..end].to_string()
    };

    // 读取并丢弃所有HTTP请求头（直到空行）
    let headers_start = Instant::now();
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).await? == 0 {
            break;
        }
        if header_line.trim().is_empty() {
            break; // 空行表示请求头结束
        }
    }
    tracing::info!(elapsed_ms = headers_start.elapsed().as_millis(), path = %path, "HTTP读取请求头耗时");

    // API 请求处理
    if path.starts_with("/api/") {
        handle_api_request(&mut stream, &path, sqlite).await?;
        return Ok(());
    }

    // 静态文件处理
    let file_path = if path == "/" {
        root.join("index.html")
    } else {
        let clean = path.trim_start_matches('/');
        if clean.contains("..") || clean.starts_with('/') {
            send_response(&mut stream, 403, "text/plain", "禁止访问".as_bytes()).await?;
            return Ok(());
        }
        root.join(clean)
    };

    if !file_path.exists() || !file_path.is_file() {
        send_response(&mut stream, 404, "text/plain", "未找到".as_bytes()).await?;
        return Ok(());
    }

    match std::fs::read(&file_path) {
        Ok(content) => {
            let content_type = guess_content_type(&file_path);
            send_response(&mut stream, 200, content_type, &content).await
        }
        Err(_) => send_response(&mut stream, 500, "text/plain", b"Internal Server Error").await,
    }
}

async fn handle_api_request(
    stream: &mut TcpStream,
    path: &str,
    sqlite: Option<Arc<SqliteStorage>>,
) -> Result<()> {
    // 解析路径和查询参数
    let (api_path, query_string) = {
        if let Some(idx) = path.find('?') {
            (&path[..idx], Some(&path[idx + 1..]))
        } else {
            (path, None)
        }
    };

    let query_params = parse_query_params(query_string);

    match api_path {
        "/api/vitals" => {
            handle_vitals_request(stream, &query_params, sqlite).await?;
        }
        "/api/vitals/interval" => {
            handle_vitals_interval_request(stream, &query_params, sqlite).await?;
        }
        "/api/health" => {
            let body = r#"{"status":"ok"}"#;
            send_response(stream, 200, "application/json", body.as_bytes()).await?;
        }
        _ => {
            send_response(stream, 404, "application/json", b"{\"error\":\"not found\"}").await?;
        }
    }
    Ok(())
}

async fn handle_vitals_interval_request(
    stream: &mut TcpStream,
    query_params: &[(String, String)],
    sqlite: Option<Arc<SqliteStorage>>,
) -> Result<()> {
    let sqlite = match sqlite {
        Some(s) => s,
        None => {
            send_response(stream, 500, "application/json", r#"{"error":"数据库不可用"}"#.as_bytes()).await?;
            return Ok(());
        }
    };

    let bed_no = query_params.iter()
        .find(|(k, _)| k == "bed_no")
        .and_then(|(_, v)| v.parse::<u16>().ok());

    // 解析时间参数：格式错误时直接返回 400，禁止静默回退默认时间范围
    let start = match parse_time_param(&query_params, "start") {
        Ok(v) => v,
        Err(msg) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, msg);
            send_response(stream, 400, "application/json", error_json.as_bytes()).await?;
            return Ok(());
        }
    };

    let end = match parse_time_param(&query_params, "end") {
        Ok(v) => v,
        Err(msg) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, msg);
            send_response(stream, 400, "application/json", error_json.as_bytes()).await?;
            return Ok(());
        }
    };

    let interval = query_params.iter()
        .find(|(k, _)| k == "interval")
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(30);

    let bed_no = match bed_no {
        Some(n) => n,
        None => {
            send_response(stream, 400, "application/json", r#"{"error":"床位号参数缺失"}"#.as_bytes()).await?;
            return Ok(());
        }
    };

    use std::time::Instant;
    let query_start = Instant::now();
    
    let vitals = sqlite.get_vitals_at_interval(bed_no, start, end, interval).await
        .map_err(|e| anyhow::anyhow!("{}", e));
    
    let query_elapsed = query_start.elapsed();
    tracing::info!(bed_no = bed_no, interval = interval, elapsed_ms = query_elapsed.as_millis(), "查询体征数据耗时");

    match vitals {
        Ok(vitals) => {
            let json_start = Instant::now();
            let json = serde_json::to_string(&vitals).unwrap_or_else(|_| "[]".to_string());
            let json_elapsed = json_start.elapsed();
            tracing::info!(count = vitals.len(), json_ms = json_elapsed.as_millis(), "JSON序列化耗时");
            send_response(stream, 200, "application/json", json.as_bytes()).await?;
        }
        Err(e) => {
            let error_json = format!(r#"{{"error":"查询体征数据失败: {}"}}"#, e);
            send_response(stream, 500, "application/json", error_json.as_bytes()).await?;
        }
    }
    Ok(())
}

async fn handle_vitals_request(
    stream: &mut TcpStream,
    query_params: &[(String, String)],
    sqlite: Option<Arc<SqliteStorage>>,
) -> Result<()> {
    let sqlite = match sqlite {
        Some(s) => s,
        None => {
            send_response(stream, 500, "application/json", r#"{"error":"数据库不可用"}"#.as_bytes()).await?;
            return Ok(());
        }
    };

    // 解析参数
    let bed_no = query_params.iter()
        .find(|(k, _)| k == "bed_no")
        .and_then(|(_, v)| v.parse::<u16>().ok());

    // 解析时间参数：格式错误时直接返回 400，禁止静默回退默认时间范围
    // （此前静默回退为「最近24小时」，导致查询历史时间段时结果错误或为空且无任何提示）
    let start = match parse_time_param(&query_params, "start") {
        Ok(v) => v,
        Err(msg) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, msg);
            send_response(stream, 400, "application/json", error_json.as_bytes()).await?;
            return Ok(());
        }
    };

    let end = match parse_time_param(&query_params, "end") {
        Ok(v) => v,
        Err(msg) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, msg);
            send_response(stream, 400, "application/json", error_json.as_bytes()).await?;
            return Ok(());
        }
    };

    let limit = query_params.iter()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(1000);

    let bed_no = match bed_no {
        Some(n) => n,
        None => {
            send_response(stream, 400, "application/json", r#"{"error":"床位号参数缺失"}"#.as_bytes()).await?;
            return Ok(());
        }
    };

    // 查询数据
    let vitals = sqlite.get_vitals(bed_no, start, end, limit).await
        .map_err(|e| anyhow::anyhow!("{}", e));

    match vitals {
        Ok(vitals) => {
            let json = serde_json::to_string(&vitals).unwrap_or_else(|_| "[]".to_string());
            send_response(stream, 200, "application/json", json.as_bytes()).await?;
        }
        Err(e) => {
            let error_json = format!(r#"{{"error":"查询体征数据失败: {}"}}"#, e);
            send_response(stream, 500, "application/json", error_json.as_bytes()).await?;
        }
    }
    Ok(())
}

/// 解析时间类查询参数（RFC3339 格式）。
/// - 参数不存在：返回 Ok(None)，由调用方决定默认值；
/// - 参数存在但格式错误：返回 Err，调用方必须返回 400 错误，
///   绝不允许静默回退默认时间范围（否则查询结果与用户所选范围不符且无从排查）。
fn parse_time_param(
    query_params: &[(String, String)],
    name: &str,
) -> std::result::Result<Option<DateTime<Utc>>, String> {
    match query_params.iter().find(|(k, _)| k == name) {
        None => Ok(None),
        Some((_, v)) => DateTime::parse_from_rfc3339(v)
            .map(|dt| Some(dt.with_timezone(&Utc)))
            .map_err(|e| format!("{} 参数格式错误（要求 RFC3339，如 2026-10-28T06:00:00Z）: {}={}", name, v, e)),
    }
}

fn parse_query_params(query: Option<&str>) -> Vec<(String, String)> {
    match query {
        Some(q) => q
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?.url_decode().ok()?;
                let value = parts.next().unwrap_or("").url_decode().ok()?;
                Some((key, value))
            })
            .collect(),
        None => Vec::new(),
    }
}

trait UrlDecode {
    fn url_decode(&self) -> std::result::Result<String, ()>;
}

impl UrlDecode for str {
    fn url_decode(&self) -> std::result::Result<String, ()> {
        let mut result = String::with_capacity(self.len());
        let mut chars = self.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                        continue;
                    }
                }
                result.push('%');
                result.push_str(&hex);
            } else if c == '+' {
                result.push(' ');
            } else {
                result.push(c);
            }
        }
        Ok(result)
    }
}

async fn send_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    use std::time::Instant;
    let send_start = Instant::now();
    
    let status_text = match status {
        200 => "OK",
        400 => "参数错误",
        403 => "禁止访问",
        404 => "未找到",
        405 => "方法不允许",
        500 => "服务器内部错误",
        _ => "OK",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        status,
        status_text,
        content_type,
        body.len()
    );

    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    
    tracing::info!(elapsed_ms = send_start.elapsed().as_millis(), status = status, body_len = body.len(), "HTTP响应发送耗时");
    Ok(())
}

fn guess_content_type(path: &Path) -> &str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}
