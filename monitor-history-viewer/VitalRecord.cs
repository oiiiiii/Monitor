using System;
using System.Globalization;
using System.Text.Json.Serialization;

namespace MonitorHistoryViewer;

/// <summary>
/// 体征数据记录
/// </summary>
public class VitalRecord
{
    /// <summary>星期简称（与 DateTime.CurrentCulture 无关，固定中文）</summary>
    private static readonly string[] WeekNames = { "周日", "周一", "周二", "周三", "周四", "周五", "周六" };

    /// <summary>缓存解析后的时间戳（避免分组与格式化时重复解析字符串）</summary>
    private DateTime? _parsedTs;

    [JsonPropertyName("ts")]
    public string Ts { get; set; } = "";

    [JsonPropertyName("bed_no")]
    public int BedNo { get; set; }

    [JsonPropertyName("heart_rate")]
    public int? HeartRate { get; set; }

    [JsonPropertyName("spo2")]
    public int? Spo2 { get; set; }

    [JsonPropertyName("resp_rate")]
    public int? RespRate { get; set; }

    [JsonPropertyName("nibp_sys")]
    public int? NibpSys { get; set; }

    [JsonPropertyName("nibp_dia")]
    public int? NibpDia { get; set; }

    [JsonPropertyName("nibp_map")]
    public int? NibpMap { get; set; }

    [JsonPropertyName("ibp_sys")]
    public int? IbpSys { get; set; }

    [JsonPropertyName("ibp_dia")]
    public int? IbpDia { get; set; }

    [JsonPropertyName("ibp_map")]
    public int? IbpMap { get; set; }

    [JsonPropertyName("pulse_rate")]
    public int? PulseRate { get; set; }

    [JsonIgnore]
    public bool NibpIsFilled { get; set; }

    public string TimeDisplay => FormatTime(Ts);

    /// <summary>
    /// 日期分组标题（仅年月日 + 星期，如「10月28日 周三」）。
    /// 用于 DataGrid 按日期分组，解决跨日数据只显示 HH:mm 时顺序混乱的问题。
    /// </summary>
    [JsonIgnore]
    public string DateGroup
    {
        get
        {
            var dt = ParsedTs;
            if (dt == DateTime.MinValue) return "未知日期";
            return $"{dt.Month}月{dt.Day}日 {WeekNames[(int)dt.DayOfWeek]}";
        }
    }

    /// <summary>解析后的本地时间戳（解析失败返回 DateTime.MinValue）</summary>
    [JsonIgnore]
    private DateTime ParsedTs => _parsedTs ??= ParseTs(Ts);

    /// <summary>解析时间戳字符串，兼容 RFC3339 / ISO8601 等后端常见格式</summary>
    private static DateTime ParseTs(string ts)
    {
        try
        {
            if (string.IsNullOrEmpty(ts)) return DateTime.MinValue;
            if (DateTimeOffset.TryParse(ts, CultureInfo.InvariantCulture, DateTimeStyles.None, out var dto))
                return dto.LocalDateTime;
            if (DateTime.TryParse(ts, CultureInfo.InvariantCulture, DateTimeStyles.None, out var dt))
                return dt;
        }
        catch { }
        return DateTime.MinValue;
    }

    public string HeartRateDisplay => HeartRate?.ToString() ?? "-";

    public string Spo2Display
    {
        get
        {
            if (Spo2.HasValue)
            {
                if (PulseRate.HasValue)
                    return $"{Spo2}({PulseRate})";
                return Spo2.Value.ToString();
            }
            return "-";
        }
    }

    public string RespRateDisplay => RespRate?.ToString() ?? "-";

    public string NibpDisplay
    {
        get
        {
            if (NibpSys.HasValue && NibpDia.HasValue)
                return $"{NibpSys}/{NibpDia}";
            return "";
        }
    }

    public string IbpDisplay
    {
        get
        {
            if (IbpSys.HasValue && IbpDia.HasValue)
                return $"{IbpSys}/{IbpDia}";
            return "-";
        }
    }

    private static string FormatTime(string ts)
    {
        try
        {
            if (string.IsNullOrEmpty(ts)) return "-";
            var dt = DateTime.Parse(ts);
            return dt.ToString("HH:mm");
        }
        catch
        {
            return ts;
        }
    }
}
