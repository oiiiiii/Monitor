using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace MonitorLauncher
{
    public class MonitorProcessManager
    {
        private Process? _process;
        private readonly string _exePath;
        private readonly string _configPath;
        private readonly string _workingDir;
        private readonly string _logDir;
        private StreamWriter? _logWriter;
        private bool _disposed;

        /// <summary>用户是否主动请求了停止（Stop/Dispose），用于看门狗区分意外退出</summary>
        public bool StopRequested { get; private set; }

        public bool IsRunning
        {
            get
            {
                if (_process == null) return false;
                try
                {
                    return !_process.HasExited;
                }
                catch
                {
                    return false;
                }
            }
        }

        public event EventHandler? ProcessExited;
        public event EventHandler<string>? LogReceived;

        public MonitorProcessManager(string exePath, string configPath, string workingDir)
        {
            _exePath = exePath;
            _configPath = configPath;
            _workingDir = workingDir;
            _logDir = Path.Combine(workingDir, "logs");
        }

        public bool Start(bool enableLogging = true)
        {
            try
            {
                if (IsRunning) return true;

                StopRequested = false;

                var fullExePath = Path.Combine(_workingDir, _exePath);
                var fullConfigPath = Path.Combine(_workingDir, _configPath);

                // 日志初始化提前：即使可执行文件缺失也要留下现场，便于现场排查
                if (enableLogging)
                {
                    Directory.CreateDirectory(_logDir);
                    CleanupOldLogs(14, 100);
                    var logFile = Path.Combine(_logDir, $"monitor_{DateTime.Now:yyyyMMdd_HHmmss}.log");
                    _logWriter = new StreamWriter(logFile, false, Encoding.UTF8)
                    {
                        AutoFlush = true
                    };
                    LogMessage($"[启动] 日志文件: {logFile}");
                    LogMessage($"[启动] 工作目录: {_workingDir}");
                    LogMessage($"[启动] 命令行: {fullExePath} --config \"{fullConfigPath}\"");
                    LogMessage("================================================================");
                }

                if (!File.Exists(fullExePath))
                {
                    LogMessage($"[错误] 找不到可执行文件: {fullExePath}");
                    return false;
                }

                _process = new Process
                {
                    StartInfo = new ProcessStartInfo
                    {
                        FileName = fullExePath,
                        Arguments = $"--config \"{fullConfigPath}\"",
                        WorkingDirectory = _workingDir,
                        UseShellExecute = false,
                        // 无控制台运行（避免在无桌面会话下创建控制台失败）；
                        // 优雅停止通过 data\shutdown.request 信号文件通知 Rust 端
                        CreateNoWindow = true,
                        RedirectStandardOutput = true,
                        RedirectStandardError = true,
                        StandardOutputEncoding = Encoding.UTF8,
                        StandardErrorEncoding = Encoding.UTF8
                    },
                    EnableRaisingEvents = true
                };

                _process.Exited += (s, e) =>
                {
                    LogMessage("[退出] 监控服务已退出");
                    ProcessExited?.Invoke(this, EventArgs.Empty);
                };

                _process.Start();

                // 后台读取 stdout / stderr（各自读到 EOF 为止）
                if (enableLogging)
                {
                    _ = ReadStreamAsync(_process.StandardOutput, "[标准输出] ");
                    _ = ReadStreamAsync(_process.StandardError, "[错误] ");
                }

                LogMessage($"[启动] 监控服务已启动 (PID: {_process.Id})");
                return true;
            }
            catch (Exception ex)
            {
                LogMessage($"[错误] 启动失败: {ex.Message}");
                return false;
            }
        }

        private async Task ReadStreamAsync(StreamReader reader, string prefix)
        {
            try
            {
                string? line;
                while ((line = await reader.ReadLineAsync()) != null)
                {
                    if (_disposed) break;
                    LogMessage($"{prefix}{line}");
                }
            }
            catch
            {
            }
        }

        private void LogMessage(string message)
        {
            var timestamp = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss.fff");
            var logLine = $"[{timestamp}] {message}";

            // 写入文件
            if (_logWriter != null && !_disposed)
            {
                try
                {
                    _logWriter.WriteLine(logLine);
                }
                catch
                {
                }
            }

            // 通知界面
            if (!_disposed)
            {
                LogReceived?.Invoke(this, logLine);
            }
        }

        public void Stop()
        {
            try
            {
                StopRequested = true;

                if (_process != null && !_process.HasExited)
                {
                    LogMessage("[停止] 正在优雅停止监控服务...");
                    if (!TryGracefulStop(_process, 5000))
                    {
                        LogMessage("[停止] 优雅停止超时，强制结束进程");
                        _process.Kill();
                        _process.WaitForExit(5000);
                    }
                    else
                    {
                        LogMessage("[停止] 监控服务已优雅退出");
                    }
                    _process.Dispose();
                    _process = null;
                }
            }
            catch (Exception ex)
            {
                LogMessage($"[错误] 停止服务时出错: {ex.Message}");
                _process = null;
            }
        }

        /// <summary>
        /// 优雅停止：写入 data\shutdown.request 信号文件，Rust 端每 500ms 轮询后自行退出。
        /// 不依赖控制台事件，因此子进程可以无控制台运行（CreateNoWindow=true）。
        /// </summary>
        private bool TryGracefulStop(Process process, int timeoutMs)
        {
            var signalPath = Path.Combine(_workingDir, "data", "shutdown.request");
            try
            {
                var dir = Path.GetDirectoryName(signalPath);
                if (!string.IsNullOrEmpty(dir))
                {
                    Directory.CreateDirectory(dir);
                }
                File.WriteAllText(signalPath, DateTime.Now.ToString("O"));

                bool exited = process.WaitForExit(timeoutMs);
                if (!exited)
                {
                    TryDeleteSignalFile(signalPath);
                }
                return exited;
            }
            catch (Exception ex)
            {
                LogMessage($"[错误] 发送停止信号失败: {ex.Message}");
                TryDeleteSignalFile(signalPath);
                return false;
            }
        }

        private static void TryDeleteSignalFile(string path)
        {
            try
            {
                if (File.Exists(path)) File.Delete(path);
            }
            catch
            {
            }
        }

        /// <summary>清理日志目录：删除超过 maxAgeDays 天的日志，最多保留 maxFiles 个</summary>
        public void CleanupOldLogs(int maxAgeDays = 14, int maxFiles = 100)
        {
            try
            {
                if (!Directory.Exists(_logDir)) return;

                var files = new DirectoryInfo(_logDir)
                    .GetFiles("monitor_*.log")
                    .OrderBy(f => f.LastWriteTime)
                    .ToList();

                var cutoff = DateTime.Now.AddDays(-maxAgeDays);
                int deleted = 0;

                // 先删过期的
                foreach (var f in files.Where(f => f.LastWriteTime < cutoff).ToList())
                {
                    f.Delete();
                    files.Remove(f);
                    deleted++;
                }

                // 再按数量删最旧的
                while (files.Count >= maxFiles && files.Count > 0)
                {
                    files[0].Delete();
                    files.RemoveAt(0);
                    deleted++;
                }

                if (deleted > 0)
                {
                    LogMessage($"[日志] 已清理 {deleted} 个旧日志文件");
                }
            }
            catch (Exception ex)
            {
                LogMessage($"[日志] 清理旧日志失败: {ex.Message}");
            }
        }

        public void Dispose()
        {
            _disposed = true;
            StopRequested = true;

            try
            {
                _logWriter?.Flush();
                _logWriter?.Dispose();
                _logWriter = null;

                if (_process != null && !_process.HasExited)
                {
                    if (!TryGracefulStop(_process, 3000))
                    {
                        _process.Kill();
                        _process.WaitForExit(3000);
                    }
                    _process.Dispose();
                    _process = null;
                }
            }
            catch
            {
            }
        }
    }
}
