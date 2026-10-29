#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// 桌面客户端配置：指向局域网内任意一台后端服务器即可多机共享数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// 服务器 IP / 主机名
    pub host: String,
    /// Web 页面端口（默认 3000；前端会自动推导 WebSocket 端口 8080）
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    /// 启动后是否全屏显示（监护工位建议开启）
    #[serde(default)]
    pub fullscreen: bool,
}

fn default_http_port() -> u16 {
    3000
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    // 用 LocalAppData 而非漫游的 AppData：服务器地址是"这台机器"的配置，
    // 域账户环境下不应随账号漫游到其他机器
    let base = app.path().app_local_data_dir().ok()?;
    Some(base.join("config.json"))
}

fn load_config(app: &AppHandle) -> Option<ShellConfig> {
    let path = config_path(app)?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_config(app: &AppHandle, cfg: &ShellConfig) -> Result<(), String> {
    let path = config_path(app).ok_or("无法获取配置目录")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn app_icon() -> Image<'static> {
    Image::from_bytes(include_bytes!("../../logo.ico")).expect("内置图标无效")
}

/// 打开主监护窗口前预检：服务器不可达时提示并回到设置页，避免 WebView2 假死
async fn server_reachable(host: &str, port: u16) -> bool {
    let addr = format!("{}:{}", host.trim(), port);
    match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

/// 打开主监护窗口（指向局域网服务器）
fn open_main_window(app: &AppHandle, cfg: &ShellConfig) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
    let url = format!("http://{}:{}", cfg.host.trim(), cfg.http_port);
    let parsed = url.parse::<tauri::Url>().map_err(|e| e.to_string())?;

    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(parsed))
        .title("多床位心电监护系统")
        .inner_size(1600.0, 900.0)
        .min_inner_size(1024.0, 600.0)
        .maximized(true);

    if cfg.fullscreen {
        builder = builder.fullscreen(true);
    }

    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

/// 打开服务器设置窗口（本地内置页面）
fn open_setup_window(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("setup") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "setup", WebviewUrl::App("setup.html".into()))
        .title("服务器设置")
        .inner_size(520.0, 420.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn start_window(app: &AppHandle) {
    match load_config(app) {
        Some(cfg) => {
            let handle = app.clone();
            let cfg2 = cfg.clone();
            // 异步预检，避免阻塞主线程
            tauri::async_runtime::spawn(async move {
                if server_reachable(&cfg2.host, cfg2.http_port).await {
                    let h = handle.clone();
                    let h2 = h.clone();
                    let _ = h.run_on_main_thread(move || {
                        if let Err(e) = open_main_window(&h2, &cfg2) {
                            eprintln!("打开主窗口失败: {e}");
                            let _ = open_setup_window(&h2);
                        }
                    });
                } else {
                    let h = handle.clone();
                    let h2 = h.clone();
                    let _ = h.run_on_main_thread(move || {
                        let _ = native_dialog::DialogBuilder::message()
                            .set_level(native_dialog::MessageLevel::Warning)
                            .set_title("服务器无法连接")
                            .set_text(format!(
                                "无法连接到服务器 {}:{}。\n\n请确认：\n1. 后端服务已启动\n2. 服务器地址和端口正确\n3. 网络连接正常",
                                cfg2.host, cfg2.http_port
                            ))
                            .alert()
                            .show();
                        let _ = open_setup_window(&h2);
                    });
                }
            });
        }
        None => {
            let _ = open_setup_window(app);
        }
    }
}

/// 托盘菜单：显示窗口 / 服务器设置 / 退出
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "重新加载").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "服务器设置").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show, &reload, &settings, &quit])
        .build()?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app_icon())
        .tooltip("多床位心电监护系统")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                } else {
                    start_window(app);
                }
            }
            "reload" => {
                // 服务器断线后 WebView2 停留在错误页，重建主窗口实现自愈
                if let Some(cfg) = load_config(app) {
                    let _ = open_main_window(app, &cfg);
                }
            }
            "settings" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.close();
                }
                let _ = open_setup_window(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[tauri::command]
fn get_config(app: AppHandle) -> Option<ShellConfig> {
    load_config(&app)
}

#[tauri::command]
async fn connect_server(app: AppHandle, config: ShellConfig) -> Result<(), String> {
    let cfg = ShellConfig {
        host: config.host.trim().to_string(),
        http_port: config.http_port,
        fullscreen: config.fullscreen,
    };
    if cfg.host.is_empty() {
        return Err("服务器地址不能为空".into());
    }
    // 保存前先探测连通性，不可达时给出明确提示而不是打开假死窗口
    if !server_reachable(&cfg.host, cfg.http_port).await {
        return Err(format!(
            "无法连接到 {}:{}，请确认服务已启动且地址端口正确",
            cfg.host, cfg.http_port
        ));
    }
    save_config(&app, &cfg)?;
    if let Some(w) = app.get_webview_window("setup") {
        let _ = w.close();
    }
    open_main_window(&app, &cfg)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 单实例：重复启动时聚焦已有主窗口
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![get_config, connect_server])
        .setup(|app| {
            let handle = app.handle().clone();
            setup_tray(&handle)?;
            start_window(&handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 monitor-shell 失败");
}
