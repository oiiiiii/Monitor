using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using Microsoft.Win32;

namespace MonitorLauncher
{
    public class AppConfig
    {
        public string MonitorExePath { get; set; } = "monitor.exe";
        public string ConfigFilePath { get; set; } = "config.toml";
        public string WebUrl { get; set; } = "http://localhost:3000";
        public int TcpPort { get; set; } = 9000;
        public int WsPort { get; set; } = 8080;
        public int HttpPort { get; set; } = 3000;
        public Dictionary<string, int> IpToBed { get; set; } = new();
        public RetentionConfig Retention { get; set; } = new();
        public bool AutoStartMonitor { get; set; } = true;
        public bool MinimizeToTray { get; set; } = true;
        public bool AutoStartSystem { get; set; } = false;
        public bool EnableLogging { get; set; } = true;
        public string LogLevel { get; set; } = "info";
    }

    public class RetentionConfig
    {
        public int VitalHours { get; set; } = 24;
        public int WaveMinutes { get; set; } = 5;
        public int AlarmMinutes { get; set; } = 5;
    }

    public static class ConfigManager
    {
        private static string ConfigFile
        {
            get
            {
                var exePath = System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName;
                var dir = !string.IsNullOrEmpty(exePath) ? Path.GetDirectoryName(exePath) : AppContext.BaseDirectory;
                return Path.Combine(dir, "launcher-config.json");
            }
        }

        public static AppConfig Load()
        {
            try
            {
                if (File.Exists(ConfigFile))
                {
                    var json = File.ReadAllText(ConfigFile);
                    var config = JsonSerializer.Deserialize<AppConfig>(json, new JsonSerializerOptions
                    {
                        PropertyNameCaseInsensitive = true,
                        WriteIndented = true
                    });
                    if (config != null)
                    {
                        if (config.Retention == null)
                        {
                            config.Retention = new RetentionConfig();
                        }
                        if (string.IsNullOrEmpty(config.LogLevel))
                        {
                            config.LogLevel = "info";
                        }
                        return config;
                    }
                }
            }
            catch
            {
            }
            return new AppConfig();
        }

        public static void Save(AppConfig config)
        {
            try
            {
                var json = JsonSerializer.Serialize(config, new JsonSerializerOptions
                {
                    WriteIndented = true
                });
                File.WriteAllText(ConfigFile, json);
            }
            catch
            {
            }
        }

        public static void GenerateTomlConfig(AppConfig config)
        {
            try
            {
                var tomlPath = Path.Combine(AppContext.BaseDirectory, config.ConfigFilePath);

                var logLevel = string.IsNullOrEmpty(config.LogLevel) ? "info" : config.LogLevel.ToLower();

                var lines = new List<string>
                {
                    "# ========================================",
                    "#  监护仪监控系统 - 配置文件",
                    "# ========================================",
                    "",
                    "# 监听地址",
                    "listen_addr = \"0.0.0.0\"",
                    "",
                    "# 设备 TCP 连接端口",
                    $"tcp_port = {config.TcpPort}",
                    "",
                    "# WebSocket 端口（前端连接）",
                    $"ws_port = {config.WsPort}",
                    "",
                    "# HTTP 静态文件端口",
                    $"http_port = {config.HttpPort}",
                    "",
                    "# 最大连接数",
                    "max_connections = 64",
                    "",
                    "# 数据目录",
                    "data_dir = \"./data\"",
                    "",
                    "# 日志目录",
                    "log_dir = \"./logs\"",
                    "",
                    "# Web 前端文件目录",
                    "web_dir = \"./web\"",
                    "",
                    "# 日志级别: trace, debug, info, warn, error",
                    $"log_level = \"{logLevel}\"",
                    "",
                    "# ========================================",
                    "#  数据保留设置",
                    "# ========================================",
                    "# 体征数据保留小时数",
                    $"vital_retention_hours = {config.Retention.VitalHours}",
                    "",
                    "# 波形数据保留分钟数",
                    $"wave_retention_minutes = {config.Retention.WaveMinutes}",
                    "",
                    "# 报警信息保留分钟数",
                    $"alarm_retention_minutes = {config.Retention.AlarmMinutes}",
                    "",
                    "# 心跳间隔（秒）",
                    "heartbeat_interval_secs = 30",
                    "",
                    "# ========================================",
                    "#  IP 到床位号映射（手动配置）",
                    "# ========================================",
                    "[ip_to_bed]",
                };

                foreach (var kvp in config.IpToBed)
                {
                    lines.Add($"\"{kvp.Key}\" = {kvp.Value}");
                }

                if (config.IpToBed.Count == 0)
                {
                    lines.Add("# 示例：");
                    lines.Add("# \"10.10.12.63\" = 1");
                    lines.Add("# \"10.10.12.65\" = 2");
                }

                lines.Add("");

                File.WriteAllLines(tomlPath, lines);
            }
            catch
            {
            }
        }

        private const string StartupRegistryKey = @"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
        private const string AppName = "MonitorLauncher";

        public static bool IsAutoStartEnabled()
        {
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(StartupRegistryKey, false);
                return key?.GetValue(AppName) != null;
            }
            catch
            {
                return false;
            }
        }

        public static void SetAutoStart(bool enable)
        {
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(StartupRegistryKey, true);
                if (key == null) return;

                if (enable)
                {
                    var exePath = System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName;
                    if (!string.IsNullOrEmpty(exePath))
                    {
                        key.SetValue(AppName, $"\"{exePath}\"");
                    }
                }
                else
                {
                    key.DeleteValue(AppName, false);
                }
            }
            catch
            {
            }
        }
    }
}