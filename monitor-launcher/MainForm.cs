using System;
using System.Drawing;
using System.Windows.Forms;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Sockets;
using System.Threading;
using System.Collections.Generic;

namespace MonitorLauncher
{
    public partial class MainForm : Form
    {
        private readonly AppConfig _config;
        private readonly MonitorProcessManager _processManager;
        private NotifyIcon? _trayIcon;
        private ContextMenuStrip? _trayMenu;
        private ToolStripMenuItem? _statusMenuItem;
        private ToolStripMenuItem? _startStopMenuItem;
        private ListBox? _logListBox;
        private readonly object _logLock = new();
        private const string MutexName = "MonitorLauncher_SingleInstance_Mutex";

        // 看门狗状态
        private int _restartCount;
        private DateTime _lastStartTime;
        private System.Threading.Timer? _watchdogTimer;
        private bool _exiting;

        // 健康探测状态
        private System.Threading.Timer? _healthTimer;
        private int _healthFailCount;
        private static readonly HttpClient _healthClient = new() { Timeout = TimeSpan.FromSeconds(3) };

        public MainForm()
        {
            // 单例检查
            bool createdNew;
            _singleInstanceMutex = new Mutex(true, MutexName, out createdNew);

            if (!createdNew)
            {
                // 尝试激活已有实例
                MessageBox.Show("程序已在运行中，正在激活已有窗口...", "提示", MessageBoxButtons.OK, MessageBoxIcon.Information);
                ActivateExistingInstance();
                Environment.Exit(0);
                return;
            }

            _config = ConfigManager.Load();
            var exePath = System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName;
            var workingDir = !string.IsNullOrEmpty(exePath) ? Path.GetDirectoryName(exePath) : AppContext.BaseDirectory;
            _processManager = new MonitorProcessManager(
                _config.MonitorExePath,
                _config.ConfigFilePath,
                workingDir
            );
            _processManager.ProcessExited += OnProcessExited;
            _processManager.LogReceived += OnLogReceived;

            // 启动时清理旧日志（14 天前 / 最多保留 100 个）
            _processManager.CleanupOldLogs(14, 100);

            InitializeComponent();
            this.Icon = GetAppIcon();
            SetupTrayIcon();
            LoadConfigToUI();

            // 启动健康探测定时器（每 10 秒）
            _healthTimer = new System.Threading.Timer(_ => _ = ProbeHealthAsync(), null, 10000, 10000);

            if (_config.AutoStartMonitor)
            {
                StartMonitor();
            }
        }

        private static Mutex? _singleInstanceMutex;

        private static void ActivateExistingInstance()
        {
            var currentProcess = Process.GetCurrentProcess();
            var processes = Process.GetProcessesByName(currentProcess.ProcessName);

            foreach (var process in processes)
            {
                if (process.Id != currentProcess.Id && process.MainWindowHandle != IntPtr.Zero)
                {
                    // 设置前台窗口
                    SetForegroundWindow(process.MainWindowHandle);
                    // 恢复窗口
                    ShowWindow(process.MainWindowHandle, SW_RESTORE);
                    break;
                }
            }
        }

        [System.Runtime.InteropServices.DllImport("user32.dll")]
        private static extern bool SetForegroundWindow(IntPtr hWnd);

        [System.Runtime.InteropServices.DllImport("user32.dll")]
        private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        private const int SW_RESTORE = 9;

        private void InitializeComponent()
        {
            this.SuspendLayout();

            this.Text = "监护仪监控系统 - 管理器";
            this.Size = new Size(750, 580);
            this.StartPosition = FormStartPosition.CenterScreen;
            this.FormBorderStyle = FormBorderStyle.FixedDialog;
            this.MaximizeBox = false;
            this.MinimizeBox = true;
            this.ShowInTaskbar = true;

            var tabControl = new TabControl
            {
                Dock = DockStyle.Fill,
                Location = new Point(10, 10)
            };

            var tabBasic = new TabPage("基本设置");
            var tabIpMapping = new TabPage("IP床位映射");
            var tabRetention = new TabPage("数据存储");
            var tabLog = new TabPage("实时日志");
            var tabAbout = new TabPage("关于");

            SetupBasicTab(tabBasic);
            SetupIpMappingTab(tabIpMapping);
            SetupRetentionTab(tabRetention);
            SetupLogTab(tabLog);
            SetupAboutTab(tabAbout);

            tabControl.TabPages.Add(tabBasic);
            tabControl.TabPages.Add(tabIpMapping);
            tabControl.TabPages.Add(tabRetention);
            tabControl.TabPages.Add(tabLog);
            tabControl.TabPages.Add(tabAbout);

            var bottomPanel = new Panel
            {
                Dock = DockStyle.Bottom,
                Height = 50
            };

            var btnSave = new Button
            {
                Text = "保存配置",
                Size = new Size(100, 30),
                Location = new Point(10, 10)
            };
            btnSave.Click += (s, e) => SaveConfig();

            var btnStart = new Button
            {
                Text = "启动监控",
                Size = new Size(100, 30),
                Location = new Point(120, 10)
            };
            btnStart.Name = "btnStart";
            btnStart.Click += (s, e) => ToggleMonitor();

            var btnOpenWeb = new Button
            {
                Text = "打开Web界面",
                Size = new Size(120, 30),
                Location = new Point(230, 10)
            };
            btnOpenWeb.Click += (s, e) => OpenWebInterface();

            var lblStatus = new Label
            {
                Text = "状态: 未运行",
                Name = "lblStatus",
                AutoSize = true,
                Location = new Point(400, 18),
                ForeColor = Color.Gray,
                Font = new Font("Microsoft YaHei UI", 9, FontStyle.Bold)
            };

            bottomPanel.Controls.Add(btnSave);
            bottomPanel.Controls.Add(btnStart);
            bottomPanel.Controls.Add(btnOpenWeb);
            bottomPanel.Controls.Add(lblStatus);

            this.Controls.Add(tabControl);
            this.Controls.Add(bottomPanel);

            this.ResumeLayout(false);
        }

        private void SetupBasicTab(TabPage tab)
        {
            var panel = new Panel { Dock = DockStyle.Fill, Padding = new Padding(20) };

            int y = 20;

            AddLabel(panel, "TCP 监听端口:", 20, y, 120);
            var txtTcpPort = new NumericUpDown
            {
                Name = "txtTcpPort",
                Location = new Point(150, y - 3),
                Size = new Size(100, 25),
                Minimum = 1,
                Maximum = 65535,
                Value = _config.TcpPort
            };
            panel.Controls.Add(txtTcpPort);
            y += 40;

            AddLabel(panel, "WebSocket 端口:", 20, y, 120);
            var txtWsPort = new NumericUpDown
            {
                Name = "txtWsPort",
                Location = new Point(150, y - 3),
                Size = new Size(100, 25),
                Minimum = 1,
                Maximum = 65535,
                Value = _config.WsPort
            };
            panel.Controls.Add(txtWsPort);
            y += 40;

            AddLabel(panel, "HTTP Web 端口:", 20, y, 120);
            var txtHttpPort = new NumericUpDown
            {
                Name = "txtHttpPort",
                Location = new Point(150, y - 3),
                Size = new Size(100, 25),
                Minimum = 1,
                Maximum = 65535,
                Value = _config.HttpPort
            };
            panel.Controls.Add(txtHttpPort);
            y += 40;

            AddLabel(panel, "Web 界面地址:", 20, y, 120);
            var txtWebUrl = new TextBox
            {
                Name = "txtWebUrl",
                Location = new Point(150, y - 3),
                Size = new Size(200, 25),
                Text = _config.WebUrl
            };
            panel.Controls.Add(txtWebUrl);
            y += 50;

            AddLabel(panel, "日志级别:", 20, y, 120);
            var cmbLogLevel = new ComboBox
            {
                Name = "cmbLogLevel",
                Location = new Point(150, y - 3),
                Size = new Size(120, 25),
                DropDownStyle = ComboBoxStyle.DropDownList
            };
            cmbLogLevel.Items.AddRange(new[] { "error", "warn", "info", "debug", "trace" });
            cmbLogLevel.SelectedItem = _config.LogLevel ?? "error";
            panel.Controls.Add(cmbLogLevel);
            y += 50;

            var chkAutoStart = new CheckBox
            {
                Name = "chkAutoStart",
                Text = "启动管理器时自动启动监控服务",
                Location = new Point(20, y),
                AutoSize = true,
                Checked = _config.AutoStartMonitor
            };
            panel.Controls.Add(chkAutoStart);
            y += 35;

            var chkMinimize = new CheckBox
            {
                Name = "chkMinimize",
                Text = "关闭窗口时最小化到任务栏",
                Location = new Point(20, y),
                AutoSize = true,
                Checked = _config.MinimizeToTray
            };
            panel.Controls.Add(chkMinimize);
            y += 35;

            var chkAutoStartSystem = new CheckBox
            {
                Name = "chkAutoStartSystem",
                Text = "开机自动启动管理器",
                Location = new Point(20, y),
                AutoSize = true,
                Checked = _config.AutoStartSystem
            };
            panel.Controls.Add(chkAutoStartSystem);
            y += 35;

            var chkEnableLog = new CheckBox
            {
                Name = "chkEnableLog",
                Text = "保存日志到文件（logs目录）",
                Location = new Point(20, y),
                AutoSize = true,
                Checked = _config.EnableLogging
            };
            panel.Controls.Add(chkEnableLog);

            tab.Controls.Add(panel);
        }

        private void SetupIpMappingTab(TabPage tab)
        {
            var panel = new Panel { Dock = DockStyle.Fill, Padding = new Padding(20) };

            var listView = new ListView
            {
                Name = "lvIpMapping",
                Location = new Point(20, 20),
                Size = new Size(380, 350),
                View = View.Details,
                FullRowSelect = true,
                GridLines = true
            };
            listView.Columns.Add("IP 地址", 200);
            listView.Columns.Add("床位号", 150);

            foreach (var kvp in _config.IpToBed)
            {
                listView.Items.Add(new ListViewItem(new[] { kvp.Key, kvp.Value.ToString() }));
            }

            var btnAdd = new Button
            {
                Text = "添加",
                Location = new Point(420, 20),
                Size = new Size(80, 30)
            };
            btnAdd.Click += (s, e) => AddIpMapping(listView);

            var btnEdit = new Button
            {
                Text = "编辑",
                Location = new Point(420, 60),
                Size = new Size(80, 30)
            };
            btnEdit.Click += (s, e) => EditIpMapping(listView);

            var btnDelete = new Button
            {
                Text = "删除",
                Location = new Point(420, 100),
                Size = new Size(80, 30)
            };
            btnDelete.Click += (s, e) => DeleteIpMapping(listView);

            panel.Controls.Add(listView);
            panel.Controls.Add(btnAdd);
            panel.Controls.Add(btnEdit);
            panel.Controls.Add(btnDelete);

            tab.Controls.Add(panel);
        }

        private void SetupRetentionTab(TabPage tab)
        {
            var panel = new Panel { Dock = DockStyle.Fill, Padding = new Padding(20) };

            int y = 30;

            AddLabel(panel, "体征数据保留小时数:", 30, y, 160);
            var numVital = new NumericUpDown
            {
                Name = "numVital",
                Location = new Point(200, y - 3),
                Size = new Size(100, 25),
                Minimum = 1,
                Maximum = 87600,
                Value = _config.Retention.VitalHours
            };
            panel.Controls.Add(numVital);
            y += 50;

            AddLabel(panel, "波形数据保留分钟数:", 30, y, 160);
            var numWave = new NumericUpDown
            {
                Name = "numWave",
                Location = new Point(200, y - 3),
                Size = new Size(100, 25),
                Minimum = 1,
                Maximum = 10080,
                Value = _config.Retention.WaveMinutes
            };
            panel.Controls.Add(numWave);
            y += 50;

            AddLabel(panel, "报警信息保留分钟数:", 30, y, 160);
            var numAlarm = new NumericUpDown
            {
                Name = "numAlarm",
                Location = new Point(200, y - 3),
                Size = new Size(100, 25),
                Minimum = 1,
                Maximum = 10080,
                Value = _config.Retention.AlarmMinutes
            };
            panel.Controls.Add(numAlarm);
            y += 50;

            var hint = new Label
            {
                Text = "提示: 系统每小时自动清理过期数据",
                Location = new Point(30, y),
                AutoSize = true,
                ForeColor = Color.Gray,
                Font = new Font("Microsoft YaHei UI", 8)
            };
            panel.Controls.Add(hint);

            tab.Controls.Add(panel);
        }

        private void SetupLogTab(TabPage tab)
        {
            var panel = new Panel { Dock = DockStyle.Fill, Padding = new Padding(10) };

            _logListBox = new ListBox
            {
                Name = "logListBox",
                Dock = DockStyle.Fill,
                IntegralHeight = false,
                Font = new Font("Consolas", 9),
                BackColor = Color.FromArgb(30, 30, 30),
                ForeColor = Color.LightGray
            };

            var topPanel = new Panel
            {
                Dock = DockStyle.Top,
                Height = 35
            };

            var btnClearLog = new Button
            {
                Text = "清空日志",
                Location = new Point(10, 5),
                Size = new Size(80, 25)
            };
            btnClearLog.Click += (s, e) =>
            {
                if (_logListBox != null)
                {
                    _logListBox.Items.Clear();
                }
            };

            var btnOpenLogFolder = new Button
            {
                Text = "打开日志目录",
                Location = new Point(100, 5),
                Size = new Size(100, 25)
            };
            btnOpenLogFolder.Click += (s, e) =>
            {
                var logDir = Path.Combine(AppContext.BaseDirectory, "logs");
                if (Directory.Exists(logDir))
                {
                    Process.Start("explorer.exe", logDir);
                }
            };

            topPanel.Controls.Add(btnClearLog);
            topPanel.Controls.Add(btnOpenLogFolder);

            panel.Controls.Add(_logListBox);
            panel.Controls.Add(topPanel);

            tab.Controls.Add(panel);
        }

        private void SetupAboutTab(TabPage tab)
        {
            var panel = new Panel { Dock = DockStyle.Fill, Padding = new Padding(20) };

            var title = new Label
            {
                Text = "监护仪监控系统",
                Font = new Font("Microsoft YaHei UI", 16, FontStyle.Bold),
                Location = new Point(20, 30),
                AutoSize = true
            };

            var version = new Label
            {
                Text = "版本: 1.0.0",
                Location = new Point(20, 70),
                AutoSize = true,
                ForeColor = Color.Gray
            };

            var desc = new Label
            {
                Text = "用于管理监护仪监控服务的启动、配置和监控。",
                Location = new Point(20, 100),
                AutoSize = true,
                MaximumSize = new Size(400, 0)
            };

            var logInfo = new Label
            {
                Text = "日志文件保存在程序目录下的 logs 文件夹中",
                Location = new Point(20, 130),
                AutoSize = true,
                ForeColor = Color.Gray,
                Font = new Font("Microsoft YaHei UI", 8)
            };

            panel.Controls.Add(title);
            panel.Controls.Add(version);
            panel.Controls.Add(desc);
            panel.Controls.Add(logInfo);

            tab.Controls.Add(panel);
        }

        private void OnLogReceived(object? sender, string logLine)
        {
            if (_logListBox == null || _logListBox.IsDisposed) return;

            if (_logListBox.InvokeRequired)
            {
                _logListBox.BeginInvoke(new Action(() => AddLogLine(logLine)));
            }
            else
            {
                AddLogLine(logLine);
            }
        }

        private void AddLogLine(string line)
        {
            if (_logListBox == null) return;

            _logListBox.Items.Add(line);

            // 限制日志数量，防止内存溢出
            while (_logListBox.Items.Count > 5000)
            {
                _logListBox.Items.RemoveAt(0);
            }

            // 自动滚动到底部
            _logListBox.TopIndex = _logListBox.Items.Count - 1;
        }

        private static void AddLabel(Control parent, string text, int x, int y, int width)
        {
            var lbl = new Label
            {
                Text = text,
                Location = new Point(x, y),
                Size = new Size(width, 20),
                TextAlign = ContentAlignment.MiddleRight
            };
            parent.Controls.Add(lbl);
        }

        private void SetupTrayIcon()
        {
            _trayMenu = new ContextMenuStrip();

            _statusMenuItem = new ToolStripMenuItem("状态: 未运行")
            {
                Enabled = false,
                ForeColor = Color.Gray
            };
            _trayMenu.Items.Add(_statusMenuItem);
            _trayMenu.Items.Add(new ToolStripSeparator());

            var openMenuItem = new ToolStripMenuItem("打开管理界面");
            openMenuItem.Click += (s, e) => ShowMainWindow();
            _trayMenu.Items.Add(openMenuItem);

            var openWebMenuItem = new ToolStripMenuItem("打开Web监控界面");
            openWebMenuItem.Click += (s, e) => OpenWebInterface();
            _trayMenu.Items.Add(openWebMenuItem);

            _startStopMenuItem = new ToolStripMenuItem("启动监控服务");
            _startStopMenuItem.Click += (s, e) => ToggleMonitor();
            _trayMenu.Items.Add(_startStopMenuItem);

            _trayMenu.Items.Add(new ToolStripSeparator());

            var exitMenuItem = new ToolStripMenuItem("退出");
            exitMenuItem.Click += (s, e) => ExitApp();
            _trayMenu.Items.Add(exitMenuItem);

            _trayIcon = new NotifyIcon
            {
                Text = "监护仪监控系统",
                Icon = GetTrayIcon(false),
                Visible = true,
                ContextMenuStrip = _trayMenu
            };
            _trayIcon.DoubleClick += (s, e) => ShowMainWindow();
        }

        private static Icon? _appIcon;

        private static Icon GetAppIcon()
        {
            if (_appIcon == null)
            {
                try
                {
                    var baseDir = AppContext.BaseDirectory;
                    var icoPath = Path.Combine(baseDir, "monitor.ico");
                    if (File.Exists(icoPath))
                    {
                        _appIcon = new Icon(icoPath);
                    }
                    else
                    {
                        var exePath = Path.Combine(baseDir, "MonitorLauncher.exe");
                        if (File.Exists(exePath))
                        {
                            _appIcon = Icon.ExtractAssociatedIcon(exePath);
                        }
                        else
                        {
                            _appIcon = SystemIcons.Application;
                        }
                    }
                }
                catch
                {
                    _appIcon = SystemIcons.Application;
                }
            }
            return _appIcon!;
        }

        private static Icon GetTrayIcon(bool running)
        {
            var baseIcon = GetAppIcon();
            if (!running)
            {
                using var bitmap = baseIcon.ToBitmap();
                using var g = Graphics.FromImage(bitmap);
                g.FillEllipse(Brushes.Gray, bitmap.Width - 8, bitmap.Height - 8, 8, 8);
                IntPtr hIcon = bitmap.GetHicon();
                var icon = Icon.FromHandle(hIcon);
                var cloned = (Icon)icon.Clone();
                icon.Dispose();
                return cloned;
            }
            return (Icon)baseIcon.Clone();
        }

        private void LoadConfigToUI()
        {
        }

        private void SaveConfig()
        {
            try
            {
                var txtTcpPort = Controls.Find("txtTcpPort", true);
                if (txtTcpPort.Length > 0 && txtTcpPort[0] is NumericUpDown tcpNum)
                    _config.TcpPort = (int)tcpNum.Value;

                var txtWsPort = Controls.Find("txtWsPort", true);
                if (txtWsPort.Length > 0 && txtWsPort[0] is NumericUpDown wsNum)
                    _config.WsPort = (int)wsNum.Value;

                var txtHttpPort = Controls.Find("txtHttpPort", true);
                if (txtHttpPort.Length > 0 && txtHttpPort[0] is NumericUpDown httpNum)
                    _config.HttpPort = (int)httpNum.Value;

                var txtWebUrl = Controls.Find("txtWebUrl", true);
                if (txtWebUrl.Length > 0 && txtWebUrl[0] is TextBox urlBox)
                    _config.WebUrl = urlBox.Text;

                var chkAutoStart = Controls.Find("chkAutoStart", true);
                if (chkAutoStart.Length > 0 && chkAutoStart[0] is CheckBox autoBox)
                    _config.AutoStartMonitor = autoBox.Checked;

                var chkMinimize = Controls.Find("chkMinimize", true);
                if (chkMinimize.Length > 0 && chkMinimize[0] is CheckBox minBox)
                    _config.MinimizeToTray = minBox.Checked;

                var chkAutoStartSystem = Controls.Find("chkAutoStartSystem", true);
                if (chkAutoStartSystem.Length > 0 && chkAutoStartSystem[0] is CheckBox sysBox)
                {
                    _config.AutoStartSystem = sysBox.Checked;
                    ConfigManager.SetAutoStart(sysBox.Checked);
                }

                var chkEnableLog = Controls.Find("chkEnableLog", true);
                if (chkEnableLog.Length > 0 && chkEnableLog[0] is CheckBox logBox)
                    _config.EnableLogging = logBox.Checked;

                var lvIpMapping = Controls.Find("lvIpMapping", true);
                if (lvIpMapping.Length > 0 && lvIpMapping[0] is ListView lv)
                {
                    _config.IpToBed.Clear();
                    foreach (ListViewItem item in lv.Items)
                    {
                        if (item.SubItems.Count >= 2 &&
                            !string.IsNullOrEmpty(item.Text) &&
                            int.TryParse(item.SubItems[1].Text, out int bed))
                        {
                            _config.IpToBed[item.Text] = bed;
                        }
                    }
                }

                var numVital = Controls.Find("numVital", true);
                if (numVital.Length > 0 && numVital[0] is NumericUpDown vNum)
                    _config.Retention.VitalHours = (int)vNum.Value;

                var numWave = Controls.Find("numWave", true);
                if (numWave.Length > 0 && numWave[0] is NumericUpDown wNum)
                    _config.Retention.WaveMinutes = (int)wNum.Value;

                var numAlarm = Controls.Find("numAlarm", true);
                if (numAlarm.Length > 0 && numAlarm[0] is NumericUpDown aNum)
                    _config.Retention.AlarmMinutes = (int)aNum.Value;

                var cmbLogLevel = Controls.Find("cmbLogLevel", true);
                if (cmbLogLevel.Length > 0 && cmbLogLevel[0] is ComboBox logLevelBox)
                    _config.LogLevel = logLevelBox.SelectedItem?.ToString() ?? "error";

                ConfigManager.Save(_config);
                ConfigManager.GenerateTomlConfig(_config);

                MessageBox.Show("配置已保存！\n如果监控服务正在运行，请重启服务以应用新配置。",
                    "提示", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"保存失败: {ex.Message}", "错误", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        private void AddIpMapping(ListView listView)
        {
            using var form = new Form
            {
                Text = "添加IP映射",
                Size = new Size(300, 160),
                StartPosition = FormStartPosition.CenterParent,
                FormBorderStyle = FormBorderStyle.FixedDialog,
                MaximizeBox = false
            };

            var lblIp = new Label { Text = "IP 地址:", Location = new Point(20, 20), Size = new Size(80, 20) };
            var txtIp = new TextBox { Location = new Point(100, 17), Size = new Size(160, 25) };

            var lblBed = new Label { Text = "床位号:", Location = new Point(20, 55), Size = new Size(80, 20) };
            var numBed = new NumericUpDown { Location = new Point(100, 52), Size = new Size(160, 25), Minimum = 1, Maximum = 999, Value = 1 };

            var btnOk = new Button { Text = "确定", Location = new Point(100, 85), Size = new Size(70, 28), DialogResult = DialogResult.OK };
            var btnCancel = new Button { Text = "取消", Location = new Point(180, 85), Size = new Size(70, 28), DialogResult = DialogResult.Cancel };

            form.Controls.Add(lblIp);
            form.Controls.Add(txtIp);
            form.Controls.Add(lblBed);
            form.Controls.Add(numBed);
            form.Controls.Add(btnOk);
            form.Controls.Add(btnCancel);

            form.AcceptButton = btnOk;
            form.CancelButton = btnCancel;

            if (form.ShowDialog(this) == DialogResult.OK)
            {
                var ip = txtIp.Text.Trim();
                if (!string.IsNullOrEmpty(ip))
                {
                    listView.Items.Add(new ListViewItem(new[] { ip, ((int)numBed.Value).ToString() }));
                }
            }
        }

        private void EditIpMapping(ListView listView)
        {
            if (listView.SelectedItems.Count == 0)
            {
                MessageBox.Show("请先选择要编辑的条目", "提示", MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }

            var item = listView.SelectedItems[0];
            using var form = new Form
            {
                Text = "编辑IP映射",
                Size = new Size(300, 160),
                StartPosition = FormStartPosition.CenterParent,
                FormBorderStyle = FormBorderStyle.FixedDialog,
                MaximizeBox = false
            };

            var lblIp = new Label { Text = "IP 地址:", Location = new Point(20, 20), Size = new Size(80, 20) };
            var txtIp = new TextBox { Location = new Point(100, 17), Size = new Size(160, 25), Text = item.Text };

            var lblBed = new Label { Text = "床位号:", Location = new Point(20, 55), Size = new Size(80, 20) };
            var numBed = new NumericUpDown
            {
                Location = new Point(100, 52),
                Size = new Size(160, 25),
                Minimum = 1,
                Maximum = 999,
                Value = int.TryParse(item.SubItems[1].Text, out int b) ? b : 1
            };

            var btnOk = new Button { Text = "确定", Location = new Point(100, 85), Size = new Size(70, 28), DialogResult = DialogResult.OK };
            var btnCancel = new Button { Text = "取消", Location = new Point(180, 85), Size = new Size(70, 28), DialogResult = DialogResult.Cancel };

            form.Controls.Add(lblIp);
            form.Controls.Add(txtIp);
            form.Controls.Add(lblBed);
            form.Controls.Add(numBed);
            form.Controls.Add(btnOk);
            form.Controls.Add(btnCancel);

            form.AcceptButton = btnOk;
            form.CancelButton = btnCancel;

            if (form.ShowDialog(this) == DialogResult.OK)
            {
                var ip = txtIp.Text.Trim();
                if (!string.IsNullOrEmpty(ip))
                {
                    item.Text = ip;
                    item.SubItems[1].Text = ((int)numBed.Value).ToString();
                }
            }
        }

        private void DeleteIpMapping(ListView listView)
        {
            if (listView.SelectedItems.Count == 0)
            {
                MessageBox.Show("请先选择要删除的条目", "提示", MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }

            if (MessageBox.Show("确定要删除选中的IP映射吗？", "确认",
                MessageBoxButtons.YesNo, MessageBoxIcon.Question) == DialogResult.Yes)
            {
                listView.SelectedItems[0].Remove();
            }
        }

        private void ToggleMonitor()
        {
            if (_processManager.IsRunning)
            {
                StopMonitor();
            }
            else
            {
                StartMonitor();
            }
        }

        private void StartMonitor()
        {
            SaveConfig();

            if (!ValidateRuntimeFiles())
            {
                return;
            }

            if (!PrecheckBeforeStart())
            {
                return;
            }

            _restartCount = 0;
            _healthFailCount = 0;
            _watchdogTimer?.Dispose();
            _watchdogTimer = null;

            if (_processManager.Start(_config.EnableLogging))
            {
                _lastStartTime = DateTime.Now;
                UpdateStatus(true);
            }
            else
            {
                MessageBox.Show(
                    $"无法启动监控服务！\n请确认 monitor.exe 存在于以下目录:\n{AppContext.BaseDirectory}",
                    "启动失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        /// <summary>启动前校验运行目录完整性：缺任何一项都列出明确清单</summary>
        private bool ValidateRuntimeFiles()
        {
            var exePath = System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName;
            var workingDir = !string.IsNullOrEmpty(exePath)
                ? Path.GetDirectoryName(exePath)!
                : AppContext.BaseDirectory;

            var missing = new List<string>();
            if (!File.Exists(Path.Combine(workingDir, _config.MonitorExePath)))
            {
                missing.Add(_config.MonitorExePath);
            }
            if (!File.Exists(Path.Combine(workingDir, _config.ConfigFilePath)))
            {
                missing.Add(_config.ConfigFilePath);
            }
            if (!Directory.Exists(Path.Combine(workingDir, "web")))
            {
                missing.Add(@"web\ 目录（前端静态资源）");
            }

            if (missing.Count == 0)
            {
                return true;
            }

            MessageBox.Show(
                "启动失败：运行目录缺少必要文件\n\n" +
                $"工作目录：{workingDir}\n\n" +
                "缺失：\n  · " + string.Join("\n  · ", missing) + "\n\n" +
                "请使用完整便携包部署，不要只复制启动器。",
                "启动失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return false;
        }

        /// <summary>启动前预检：端口占用 + 非本启动器拉起的旧进程</summary>
        private bool PrecheckBeforeStart()
        {
            var occupied = new List<string>();
            foreach (var (name, port) in new[]
            {
                ("TCP", _config.TcpPort),
                ("WebSocket", _config.WsPort),
                ("HTTP", _config.HttpPort)
            })
            {
                if (!CanBindPort(port))
                {
                    occupied.Add($"{name} 端口 {port}");
                }
            }

            var exeName = Path.GetFileNameWithoutExtension(_config.MonitorExePath);
            var orphans = string.IsNullOrEmpty(exeName)
                ? new List<Process>()
                : Process.GetProcessesByName(exeName).ToList();

            if (occupied.Count == 0 && orphans.Count == 0)
            {
                return true;
            }

            var msg = new System.Text.StringBuilder("检测到以下问题：\n\n");
            foreach (var o in occupied)
            {
                msg.AppendLine($"· {o} 已被占用");
            }
            foreach (var p in orphans)
            {
                msg.AppendLine($"· 发现已在运行的监控服务进程 (PID: {p.Id})，并非本启动器启动");
            }
            msg.AppendLine("\n是否结束旧进程并继续启动？");
            msg.AppendLine("（选择\"否\"将取消本次启动）");

            var result = MessageBox.Show(msg.ToString(), "启动前检查",
                MessageBoxButtons.YesNo, MessageBoxIcon.Warning);
            if (result != DialogResult.Yes)
            {
                return false;
            }

            // 结束旧进程
            foreach (var p in orphans)
            {
                try
                {
                    p.Kill();
                    p.WaitForExit(5000);
                }
                catch
                {
                }
                finally
                {
                    p.Dispose();
                }
            }

            // 重新检测端口
            var stillOccupied = new List<string>();
            foreach (var (name, port) in new[]
            {
                ("TCP", _config.TcpPort),
                ("WebSocket", _config.WsPort),
                ("HTTP", _config.HttpPort)
            })
            {
                if (!CanBindPort(port))
                {
                    stillOccupied.Add($"{name} 端口 {port}");
                }
            }

            if (stillOccupied.Count > 0)
            {
                MessageBox.Show(
                    "以下端口仍被占用，无法启动：\n\n" + string.Join("\n", stillOccupied) +
                    "\n\n可能被其他程序占用，请手动释放后重试。",
                    "启动失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return false;
            }

            return true;
        }

        /// <summary>尝试绑定端口以检测是否被占用（成功立即释放）</summary>
        private static bool CanBindPort(int port)
        {
            TcpListener? listener = null;
            try
            {
                listener = new TcpListener(IPAddress.Any, port);
                listener.Start();
                return true;
            }
            catch (SocketException)
            {
                return false;
            }
            finally
            {
                listener?.Stop();
            }
        }

        private void StopMonitor()
        {
            if (MessageBox.Show("确定要停止监控服务吗？", "确认",
                MessageBoxButtons.YesNo, MessageBoxIcon.Question) == DialogResult.Yes)
            {
                // 用户主动停止：取消看门狗，不触发自动重启
                _watchdogTimer?.Dispose();
                _watchdogTimer = null;
                _restartCount = 0;
                _healthFailCount = 0;

                _processManager.Stop();
                UpdateStatus(false);
            }
        }

        private void OnProcessExited(object? sender, EventArgs e)
        {
            if (this.IsHandleCreated)
            {
                this.BeginInvoke(() =>
                {
                    if (_exiting || _processManager.StopRequested)
                    {
                        // 用户主动停止或启动器退出，不自动重启
                        UpdateStatus(false);
                        return;
                    }

                    // 意外退出：稳定运行超过 60 秒则重置重试计数
                    if (DateTime.Now - _lastStartTime > TimeSpan.FromSeconds(60))
                    {
                        _restartCount = 0;
                    }

                    ScheduleRestart();
                });
            }
        }

        /// <summary>看门狗：按 5s → 15s → 60s 退避重启，连续失败 5 次后放弃</summary>
        private void ScheduleRestart()
        {
            _restartCount++;

            if (_restartCount >= 5)
            {
                SetStatus("状态: 服务多次崩溃，请检查日志", Color.Red, false);
                AddLogLine($"[{DateTime.Now:HH:mm:ss}] [看门狗] 服务连续崩溃 {_restartCount} 次，已停止自动重启，请检查日志");
                _trayIcon?.ShowBalloonTip(5000, "监护仪监控系统",
                    "监控服务多次崩溃，自动重启已停止，请检查日志！", ToolTipIcon.Error);
                return;
            }

            int delaySec = _restartCount switch
            {
                1 => 5,
                2 => 15,
                _ => 60
            };

            SetStatus($"状态: 服务异常退出，{delaySec} 秒后自动重启（第 {_restartCount} 次）", Color.DarkOrange, false);
            AddLogLine($"[{DateTime.Now:HH:mm:ss}] [看门狗] 服务意外退出，{delaySec} 秒后尝试第 {_restartCount} 次重启");

            _watchdogTimer?.Dispose();
            _watchdogTimer = new System.Threading.Timer(_ =>
            {
                if (_exiting || _processManager.StopRequested) return;
                try
                {
                    this.BeginInvoke(AttemptRestart);
                }
                catch
                {
                }
            }, null, delaySec * 1000, Timeout.Infinite);
        }

        private void AttemptRestart()
        {
            if (_exiting || _processManager.StopRequested || _processManager.IsRunning) return;

            if (_processManager.Start(_config.EnableLogging))
            {
                _lastStartTime = DateTime.Now;
                _healthFailCount = 0;
                UpdateStatus(true);
                AddLogLine($"[{DateTime.Now:HH:mm:ss}] [看门狗] 第 {_restartCount} 次重启成功");
            }
            else
            {
                // 启动失败视为又一次失败，继续退避
                AddLogLine($"[{DateTime.Now:HH:mm:ss}] [看门狗] 第 {_restartCount} 次重启失败");
                ScheduleRestart();
            }
        }

        /// <summary>健康探测：每 10 秒 GET /health（健康端口 = TCP 端口 + 100），连续失败 3 次标记异常</summary>
        private async Task ProbeHealthAsync()
        {
            if (_exiting) return;

            if (!_processManager.IsRunning)
            {
                _healthFailCount = 0;
                return;
            }

            bool ok;
            try
            {
                var healthPort = _config.TcpPort + 100;
                using var resp = await _healthClient.GetAsync($"http://127.0.0.1:{healthPort}/health");
                ok = (int)resp.StatusCode >= 200 && (int)resp.StatusCode < 300;
            }
            catch
            {
                ok = false;
            }

            if (ok)
            {
                if (_healthFailCount >= 3)
                {
                    _healthFailCount = 0;
                    try
                    {
                        this.BeginInvoke(() =>
                        {
                            if (!_exiting && _processManager.IsRunning)
                            {
                                UpdateStatus(true);
                                AddLogLine($"[{DateTime.Now:HH:mm:ss}] [健康检查] 服务已恢复正常");
                            }
                        });
                    }
                    catch
                    {
                    }
                }
                else
                {
                    _healthFailCount = 0;
                }
            }
            else
            {
                _healthFailCount++;
                if (_healthFailCount == 3)
                {
                    try
                    {
                        this.BeginInvoke(() =>
                        {
                            if (!_exiting && _processManager.IsRunning)
                            {
                                SetStatus("状态: 服务异常", Color.DarkOrange, true);
                                AddLogLine($"[{DateTime.Now:HH:mm:ss}] [健康检查] 连续 3 次探测失败，服务异常");
                            }
                        });
                    }
                    catch
                    {
                    }
                }
            }
        }

        private void UpdateStatus(bool running)
        {
            SetStatus(
                running ? "状态: 运行中" : "状态: 已停止",
                running ? Color.SeaGreen : Color.Gray,
                running);
        }

        private void SetStatus(string text, Color color, bool running)
        {
            if (InvokeRequired)
            {
                try
                {
                    this.BeginInvoke(() => SetStatus(text, color, running));
                }
                catch
                {
                }
                return;
            }

            var lblStatus = Controls.Find("lblStatus", true);
            if (lblStatus.Length > 0 && lblStatus[0] is Label lbl)
            {
                lbl.Text = text;
                lbl.ForeColor = color;
            }

            var btnStart = Controls.Find("btnStart", true);
            if (btnStart.Length > 0 && btnStart[0] is Button btn)
            {
                btn.Text = running ? "停止监控" : "启动监控";
            }

            if (_trayIcon != null)
            {
                _trayIcon.Icon = GetTrayIcon(running);
                _trayIcon.Text = $"监护仪监控系统 - {text.Replace("状态: ", "")}";
            }

            if (_statusMenuItem != null)
            {
                _statusMenuItem.Text = text;
                _statusMenuItem.ForeColor = color;
            }

            if (_startStopMenuItem != null)
            {
                _startStopMenuItem.Text = running ? "停止监控服务" : "启动监控服务";
            }
        }

        private void OpenWebInterface()
        {
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = _config.WebUrl,
                    UseShellExecute = true
                });
            }
            catch
            {
                MessageBox.Show($"无法打开浏览器: {_config.WebUrl}", "错误",
                    MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        private void ShowMainWindow()
        {
            this.Show();
            this.WindowState = FormWindowState.Normal;
            this.Activate();
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            if (_config.MinimizeToTray && e.CloseReason == CloseReason.UserClosing)
            {
                e.Cancel = true;
                this.Hide();
                _trayIcon?.ShowBalloonTip(2000, "监护仪监控系统",
                    "程序已最小化到任务栏，双击图标打开界面", ToolTipIcon.Info);
            }
            base.OnFormClosing(e);
        }

        private void ExitApp()
        {
            if (MessageBox.Show("确定要退出吗？监控服务也将一并停止。", "确认退出",
                MessageBoxButtons.YesNo, MessageBoxIcon.Question) == DialogResult.Yes)
            {
                _exiting = true;
                _watchdogTimer?.Dispose();
                _watchdogTimer = null;
                _healthTimer?.Dispose();
                _healthTimer = null;
                _processManager.Stop();
                _processManager.Dispose();
                _trayIcon?.Dispose();
                _singleInstanceMutex?.ReleaseMutex();
                _singleInstanceMutex?.Dispose();
                Application.Exit();
            }
        }
    }
}
