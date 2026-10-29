using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Net.Http;
using System.Net.Http.Json;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace MonitorHistoryViewer;

public class ApiService : IDisposable
{
    private readonly HttpClient _httpClient;
    private readonly string _baseUrl;
    private static readonly HttpClientHandler _handler = new HttpClientHandler
    {
        UseProxy = false,  // 禁用代理检测
        AutomaticDecompression = System.Net.DecompressionMethods.GZip | System.Net.DecompressionMethods.Deflate
    };

    public ApiService(string baseUrl)
    {
        _baseUrl = baseUrl.TrimEnd('/');
        _httpClient = new HttpClient(_handler);
        _httpClient.Timeout = TimeSpan.FromSeconds(30);
    }

    /// <summary>
    /// 将界面选择的本地时间转换为 UTC 并输出带 Z 后缀的 RFC3339 格式。
    /// 注意：界面上的 DateTime.Kind 是 Unspecified，直接用 "O" 格式输出不带时区后缀，
    /// 后端（Rust chrono RFC3339 解析）会解析失败并静默回退为「最近24小时」，
    /// 导致查询历史时间段时结果错误或为空——必须显式转为 UTC。
    /// </summary>
    private static string FormatUtc(DateTime localTime) => localTime.ToUniversalTime().ToString("O");

    public async Task<List<VitalRecord>> GetVitalsAsync(int bedNo, DateTime start, DateTime end, int limit = 50000)
    {
        var url = $"{_baseUrl}/api/vitals?bed_no={bedNo}&start={FormatUtc(start)}&end={FormatUtc(end)}&limit={limit}";

        try
        {
            var response = await _httpClient.GetAsync(url);
            response.EnsureSuccessStatusCode();

            var content = await response.Content.ReadAsStringAsync();

            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            };

            var records = JsonSerializer.Deserialize<List<VitalRecord>>(content, options);
            return records ?? new List<VitalRecord>();
        }
        catch (Exception ex)
        {
            throw new Exception($"获取体征数据失败: {ex.Message}");
        }
    }

    public async Task<List<VitalRecord>> GetVitalsAtIntervalAsync(int bedNo, DateTime start, DateTime end, int intervalMinutes = 30, CancellationToken cancellationToken = default)
    {
        var url = $"{_baseUrl}/api/vitals/interval?bed_no={bedNo}&start={FormatUtc(start)}&end={FormatUtc(end)}&interval={intervalMinutes}";
        var totalSw = Stopwatch.StartNew();

        try
        {
            var httpSw = Stopwatch.StartNew();
            var response = await _httpClient.GetAsync(url, cancellationToken);
            httpSw.Stop();
            System.Diagnostics.Debug.WriteLine($"[性能] HTTP请求耗时: {httpSw.ElapsedMilliseconds}ms");

            response.EnsureSuccessStatusCode();

            var contentSw = Stopwatch.StartNew();
            var content = await response.Content.ReadAsStringAsync(cancellationToken);
            contentSw.Stop();
            System.Diagnostics.Debug.WriteLine($"[性能] 读取响应耗时: {contentSw.ElapsedMilliseconds}ms, 内容长度: {content.Length}");

            var parseSw = Stopwatch.StartNew();
            var options = new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            };

            var records = JsonSerializer.Deserialize<List<VitalRecord>>(content, options);
            parseSw.Stop();
            totalSw.Stop();
            
            System.Diagnostics.Debug.WriteLine($"[性能] JSON解析耗时: {parseSw.ElapsedMilliseconds}ms, 记录数: {records?.Count ?? 0}");
            System.Diagnostics.Debug.WriteLine($"[性能] 总耗时: {totalSw.ElapsedMilliseconds}ms");

            return records ?? new List<VitalRecord>();
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception ex)
        {
            throw new Exception($"获取体征数据失败: {ex.Message}");
        }
    }

    public void Dispose()
    {
        _httpClient.Dispose();
    }
}