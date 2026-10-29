using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Media;

namespace MonitorHistoryViewer;

public partial class MainWindow : Window
{
    private ApiService? _apiService;
    private ViewerConfig _config = new ViewerConfig();
    private int _selectedBedNo = 1;
    private int _selectedIntervalMinutes = 30;
    private CancellationTokenSource? _queryCts;
    private readonly Brush _defaultStatusBrush;

    public MainWindow()
    {
        InitializeComponent();
        _defaultStatusBrush = StatusText.Foreground;
        _config = ViewerConfig.Load();
        _apiService = new ApiService(BuildBaseUrl());
        Topmost = true;
        Loaded += MainWindow_Loaded;
    }

    private string BuildBaseUrl() => $"http://{_config.Host}:{_config.Port}";

    private void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        HostTextBox.Text = _config.Host;
        PortTextBox.Text = _config.Port.ToString();

        var now = DateTime.Now;
        var start = now.AddHours(-24);
        StartDatePicker.SelectedDate = start.Date;
        StartTimeTextBox.Text = start.ToString("HH:mm");
        EndDatePicker.SelectedDate = now.Date;
        EndTimeTextBox.Text = now.ToString("HH:mm");

        QueryData();
    }

    private void SaveServer_Click(object sender, RoutedEventArgs e)
    {
        var host = HostTextBox.Text.Trim();
        if (string.IsNullOrEmpty(host)) host = "127.0.0.1";
        if (!int.TryParse(PortTextBox.Text.Trim(), out int port) || port <= 0 || port > 65535) port = 3000;

        _config.Host = host;
        _config.Port = port;
        _config.Save();

        _apiService?.Dispose();
        _apiService = new ApiService(BuildBaseUrl());
        QueryData();
    }

    private void QueryButton_Click(object sender, RoutedEventArgs e)
    {
        QueryData();
    }

    private void BedRadio_Click(object sender, RoutedEventArgs e)
    {
        if (sender is RadioButton rb)
        {
            var content = rb.Content?.ToString() ?? "";
            if (content.Contains("床"))
            {
                var bedStr = content.Replace("床", "");
                if (int.TryParse(bedStr, out int bedNo))
                {
                    _selectedBedNo = bedNo;
                    QueryData();
                }
            }
        }
    }

    private void IntervalRadio_Click(object sender, RoutedEventArgs e)
    {
        if (sender is RadioButton rb)
        {
            var content = rb.Content?.ToString() ?? "";
            if (content.Contains("分钟"))
            {
                var intervalStr = content.Replace("分钟", "");
                if (int.TryParse(intervalStr, out int interval))
                {
                    _selectedIntervalMinutes = interval;
                    QueryData();
                }
            }
        }
    }

    private bool TryGetTimeRange(out DateTime start, out DateTime end, out string error)
    {
        start = default;
        end = default;
        error = "";

        if (StartDatePicker.SelectedDate is not DateTime startDate) { error = "请选择开始日期"; return false; }
        if (EndDatePicker.SelectedDate is not DateTime endDate) { error = "请选择结束日期"; return false; }
        if (!TimeSpan.TryParse(StartTimeTextBox.Text.Trim(), out var startTime)) { error = "开始时间格式错误（HH:mm）"; return false; }
        if (!TimeSpan.TryParse(EndTimeTextBox.Text.Trim(), out var endTime)) { error = "结束时间格式错误（HH:mm）"; return false; }

        start = startDate.Date + startTime;
        end = endDate.Date + endTime;
        if (end <= start) { error = "结束时间必须晚于开始时间"; return false; }
        return true;
    }

    private async void QueryData()
    {
        _queryCts?.Cancel();
        _queryCts?.Dispose();
        var cts = new CancellationTokenSource();
        _queryCts = cts;
        var token = cts.Token;

        var totalSw = Stopwatch.StartNew();

        try
        {
            StatusText.Foreground = _defaultStatusBrush;
            StatusText.Text = "正在查询...";

            if (!TryGetTimeRange(out var start, out var end, out var rangeError))
            {
                StatusText.Text = rangeError;
                StatusText.Foreground = Brushes.Red;
                return;
            }

            var uiSw = Stopwatch.StartNew();
            DataGrid.ItemsSource = null;
            uiSw.Stop();
            Debug.WriteLine($"[性能] UI清空耗时: {uiSw.ElapsedMilliseconds}ms");

            if (_apiService == null)
            {
                var initSw = Stopwatch.StartNew();
                _apiService = new ApiService(BuildBaseUrl());
                initSw.Stop();
                Debug.WriteLine($"[性能] API服务初始化耗时: {initSw.ElapsedMilliseconds}ms");
            }

            Debug.WriteLine($"[性能] 开始HTTP请求 - bed_no={_selectedBedNo}, interval={_selectedIntervalMinutes}");
            var httpSw = Stopwatch.StartNew();
            var records = await _apiService.GetVitalsAtIntervalAsync(_selectedBedNo, start, end, _selectedIntervalMinutes, token);
            httpSw.Stop();
            Debug.WriteLine($"[性能] HTTP请求总耗时: {httpSw.ElapsedMilliseconds}ms");

            var bindSw = Stopwatch.StartNew();
            // 按日期分组展示：跨日数据（如 10月28日 23:00 后接 10月29日 00:00）插入日期标题栏，
            // 行内仍只显示 HH:mm，避免时间列过宽，同时顺序一目了然
            var view = new ListCollectionView(records);
            view.GroupDescriptions.Add(new PropertyGroupDescription(nameof(VitalRecord.DateGroup)));
            DataGrid.ItemsSource = view;
            bindSw.Stop();
            Debug.WriteLine($"[性能] DataGrid绑定耗时: {bindSw.ElapsedMilliseconds}ms");

            var textSw = Stopwatch.StartNew();
            RecordCountText.Text = $"共 {records.Count} 条记录";
            textSw.Stop();
            Debug.WriteLine($"[性能] 文本更新耗时: {textSw.ElapsedMilliseconds}ms");

            totalSw.Stop();
            Debug.WriteLine($"[性能] ====== 总耗时: {totalSw.ElapsedMilliseconds}ms ======");

            if (records.Count == 0)
            {
                StatusText.Text = "未查询到数据";
            }
            else
            {
                StatusText.Text = $"查询完成 - {_selectedBedNo}床，间隔{_selectedIntervalMinutes}分钟";
            }
        }
        catch (OperationCanceledException)
        {
            // 查询已被新查询取代，静默忽略
        }
        catch (Exception ex)
        {
            StatusText.Text = $"查询失败: {ex.Message}";
            StatusText.Foreground = Brushes.Red;
        }
    }

    protected override void OnClosed(EventArgs e)
    {
        base.OnClosed(e);
        _queryCts?.Cancel();
        _queryCts?.Dispose();
        _apiService?.Dispose();
        SingleInstanceHelper.ReleaseMutex();
    }
}

public class ViewerConfig
{
    public string Host { get; set; } = "127.0.0.1";
    public int Port { get; set; } = 3000;

    private static readonly string ConfigPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "MonitorHistoryViewer", "config.json");

    public static ViewerConfig Load()
    {
        try
        {
            if (File.Exists(ConfigPath))
            {
                var cfg = JsonSerializer.Deserialize<ViewerConfig>(File.ReadAllText(ConfigPath));
                if (cfg != null && !string.IsNullOrWhiteSpace(cfg.Host) && cfg.Port > 0)
                    return cfg;
            }
        }
        catch { }
        return new ViewerConfig();
    }

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(ConfigPath)!);
            File.WriteAllText(ConfigPath, JsonSerializer.Serialize(this));
        }
        catch { }
    }
}
