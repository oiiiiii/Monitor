# 多床位心电监护系统

一套面向病房 / ICU 的多床位心电监护系统：Rust 单进程后端负责设备接入与存储，Vue 3 前端负责大屏展示，附带 Windows 桌面客户端、守护启动器和历史数据查询工具。

支持科曼（Comen）与迈瑞（Mindray）N 系列监护仪的 **HL7 v2.6** 协议接入，实现生命体征、6 导联心电波形、分级报警的实时采集与展示。

> 本项目为实际病房环境持续运行验证过的稳定版本，已完成阶段性归档。

---

## 特性

| 能力 | 说明 |
|---|---|
| 设备接入 | 单 TCP 端口承载 8 台监护仪长连接，按源 IP 自动识别设备与床位 |
| 协议解析 | HL7 v2.6 完整解析，兼容科曼 250Hz / 迈瑞 500Hz 两种采样率与各自的编码差异 |
| 波形展示 | 扫掠式（sweep）绘制，与真实监护仪一致：新数据自左向右写入，到右边界后绕回覆盖最旧数据 |
| 数据存储 | SQLite 存体征/报警，波形按文件分段落盘，到期自动清理 |
| 实时推送 | WebSocket 推送体征与波形，端到端延迟 < 150ms |
| 无人值守 | 启动器守护进程、端口占用检查、异常退出自动拉起 |
| 历史回溯 | 按床位 + 时间区间查询，按天分组展示，跨日数据一目了然 |
| 零外部依赖 | 后端不依赖数据库服务器、消息队列、缓存服务，解压即可运行 |

## 架构

```
科曼 / 迈瑞监护仪
      │  TCP (HL7 v2.6, 帧头 \x0B 帧尾 \x1C\x0D)
      ▼
┌─────────────────────────────────────────────────┐
│  monitor-standalone（单进程）                    │
│                                                 │
│  TCP 接入 ──► 环形缓冲 ──► 消息总线 ──► 存储     │
│  9000         每设备独立     背压控制    ├ SQLite（体征/报警）│
│                                        └ 波形文件（data/wave）│
│                        │                                    │
│                        └──► WebSocket 推送 8080             │
│                        └──► HTTP 静态 + 查询 API 3000        │
└─────────────────────────────────────────────────┘
      │                        │
      ▼                        ▼
  Vue 3 前端            桌面客户端 / 历史查询工具
  （浏览器直连）         （monitor-shell / history-viewer）
```

## 目录结构

```
.
├── monitor-standalone/      后端主程序（bin 名 monitor）：TCP + 存储 + WS + HTTP
├── monitor-proto/           HL7 v2.6 协议解析：分帧、段解析、体征/波形/报警、MDC 编码映射
├── monitor-common/          公共组件：错误类型、指标、缓冲、缓存、消息总线抽象
├── monitor-test/            设备模拟器（bin 名 simulator），用于无真机联调
├── monitor-access/          微服务架构版接入层（历史架构，仅作参考，不参与生产部署）
├── monitor-consumer/        微服务架构版消费层（同上）
├── monitor-push/            微服务架构版推送层（同上）
├── frontend/                Vue 3 + TypeScript + Pinia + Vite 前端
├── monitor-shell/           Tauri 2 桌面客户端（WebView2 壳，可配置服务器地址）
├── monitor-launcher/        WinForms 启动器（.NET 8）：守护 monitor.exe、配置管理、托盘
├── monitor-history-viewer/  WPF 历史查询工具（.NET 8）
├── deploy/                  发布打包脚本与配置模板
├── docs/                    协议参考、阶段总结、架构设计稿
└── log/                     真实设备报文样例（集成测试用）
```

> `monitor-access` / `monitor-consumer` / `monitor-push` 属于早期的 RabbitMQ + TDengine + PostgreSQL 微服务架构，当前生产版本已统一为 `monitor-standalone` 单体实现。保留这三个 crate 仅为留存架构演进记录，构建生产版本时无需编译它们。

## 快速开始

### 环境要求

| 用途 | 依赖 |
|---|---|
| 后端 | Rust 1.75+（stable，MSVC toolchain） |
| 前端 | Node.js 18+ |
| 桌面工具 | .NET 8 SDK（构建）/ .NET 8 Desktop Runtime（运行） |
| 桌面客户端 | Rust + WebView2 Runtime（Win10/11 一般已自带） |

### 1. 构建后端

```bash
cargo build --release -p monitor-standalone
# 产物：target/release/monitor.exe
```

### 2. 构建前端

```bash
cd frontend
npm install
npm run build
# 产物：frontend/dist，需复制为后端 web_dir 指向的目录（默认 ./web）
```

### 3. 组装运行目录

```bash
mkdir -p run/web run/data run/logs
cp target/release/monitor.exe run/
cp -r frontend/dist/* run/web/
cp deploy/standalone/config.toml run/
cd run && ./monitor.exe
```

也可以用现成脚本一键打包（Windows PowerShell）：

```powershell
powershell -ExecutionPolicy Bypass -File deploy/build-standalone.ps1 -Version 0.1.0 -OutputDir ./release
```

### 4. 访问

浏览器打开 `http://<服务器IP>:3000` 即可看到 8 床位监护界面。

| 端口 | 用途 |
|---|---|
| 9000 | 设备 TCP 接入（监护仪连接此端口） |
| 8080 | WebSocket 实时推送 |
| 3000 | HTTP 静态页面 + 查询 API |

### 5. 构建桌面工具（可选）

```bash
# 启动器（守护进程 + 托盘 + 配置界面）
dotnet publish monitor-launcher/MonitorLauncher.csproj -c Release -r win-x64 \
  --self-contained false -p:PublishSingleFile=true \
  -p:IncludeAllContentForSelfExtract=true -o publish/MonitorPortable

# 历史查询工具
dotnet publish monitor-history-viewer/monitor-history-viewer.csproj -c Release -r win-x64 \
  --self-contained false -p:PublishSingleFile=true \
  -p:IncludeAllContentForSelfExtract=true -o publish/monitor-history-viewer

# 桌面客户端（Tauri，独立 workspace）
cd monitor-shell && cargo build --release
```

> WPF / WinForms 单文件发布必须加 `IncludeAllContentForSelfExtract=true`，否则 `favicon.ico` 等内容文件不会被打包，程序启动即崩。

## 配置

`config.toml`（完整模板见 `deploy/standalone/config.toml`）：

```toml
listen_addr = "0.0.0.0"
tcp_port = 9000          # 设备接入端口
ws_port = 8080           # WebSocket 端口
http_port = 3000         # HTTP 端口
data_dir = "./data"      # 数据目录（SQLite + 波形文件）
log_dir = "./logs"
web_dir = "./web"        # 前端静态文件目录

vital_retention_hours = 24    # 体征保留时长
wave_retention_minutes = 2    # 波形保留时长
alarm_retention_minutes = 2
heartbeat_interval_secs = 30

# 设备源 IP → 床位号映射，按现场实际 IP 修改
[ip_to_bed]
"10.10.12.65" = 1
"10.10.12.66" = 2
# ...
```

所有配置项都可用命令行参数临时覆盖：

```bash
monitor.exe --config config.toml --http-port 3001 --log-level info
```

## 查询 API

| 接口 | 说明 |
|---|---|
| `GET /api/health` | 健康检查，返回 `{"status":"ok"}` |
| `GET /api/vitals/interval?bed_no=1&start=<RFC3339>&end=<RFC3339>&interval=30` | 按时间区间取样体征数据 |
| `GET /api/vitals?bed_no=1&start=<RFC3339>&end=<RFC3339>&limit=500` | 按时间区间查询原始体征记录 |

时间参数必须为**带时区的 RFC3339** 格式（如 `2026-10-28T00:00:00Z` 或 `2026-10-28T08:00:00+08:00`）。格式错误时接口返回 `400` 并附带具体原因，不会静默回退到默认时间范围。

## 设备协议说明

- **帧格式**：`\x0B` 起始、`\x1C\x0D` 结束，MSH 段声明编码与消息类型
- **科曼（Comen）**：250Hz 采样、无符号 12 位波形（基线 2046）、UTC 时间需转东八区、只上报脉搏率不上报心率
- **迈瑞（Mindray）**：500Hz 采样、有符号整数波形（×0.001221 得 mV）、`ORU^R40` 报警消息、`99MNDRY` 私有编码
- 体征项通过 MDC 编码区分，科曼与迈瑞对 NIBP、IBP MAP、灌注指数等的编码不一致，已在 `monitor-proto/src/vital.rs` 中做兼容映射

详见 `docs/HL7协议参考.md`。

## 数据存储

- `data/monitor.db`：SQLite，存生命体征与报警事件
- `data/wave/*.bin`：波形二进制文件，按床位 + 时间段分片，到期自动清理

体征与波形均为「定长保留 + 定期清理」策略，长期运行磁盘占用可控。

## 已知限制

- 波形仅持久化最近数分钟（`wave_retention_minutes`），不支持任意历史波形回放
- 设备识别依赖源 IP，同一 IP 变更需重启服务或修改 `ip_to_bed`
- 前端图表基于 Canvas 手绘，未适配移动端小屏
- WebSocket 未做鉴权，仅适用于内网部署

## 文档

| 文件 | 内容 |
|---|---|
| `docs/HL7协议参考.md` | 两份设备真实报文的全字段拆解，协议实现的权威依据 |
| `docs/设计文档-目标架构.md` | 早期架构设计稿（微服务 + 时序库版本），与当前单体实现存在差异，仅作设计思路参考 |
| `docs/第X阶段工作总结.md` | 分阶段开发过程记录与问题复盘 |
| `deploy/standalone/` | 生产配置模板与启动器配置样例 |

## 许可

MIT License，详见 [LICENSE](LICENSE)。
