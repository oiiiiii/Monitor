// 类型定义

export const ECG_LEADS = ['I', 'II', 'III', 'aVR', 'aVL', 'aVF'] as const
export type ECGLead = typeof ECG_LEADS[number]

// 设备信息
export interface DeviceInfo {
  device_id: string
  device_type: string
  model: string
  bed_no: number
  patient_id: string
  patient_name: string
}

// 设备信息
export interface DeviceInfo {
  device_ip: string
  vendor: string
  model: string
  serial: string
  bed_no: number
}

// 生命体征数据
export interface VitalSignsData {
  bed_no: number
  device_id: string
  device: DeviceInfo
  sample_time: string
  heart_rate: number | null
  resp_rate: number | null
  spo2: number | null
  pi: number | null
  pulse_rate: number | null
  nibp_sys: number | null
  nibp_dia: number | null
  nibp_map: number | null
  ibp_sys: number | null
  ibp_dia: number | null
  ibp_map: number | null
  ibp_rate: number | null
  temperature: number | null
}

// 波形数据
export interface WaveformData {
  bed_no: number
  device_id: string
  lead: string
  sample_time: string
  sample_rate: number
  samples: number[]
  updateTime?: number // 用于触发重绘的时间戳
  totalSamples?: number // 累计接收的采样点总数（用于扫掠式绘制的绝对定位）
}

// 报警优先级
export type AlarmPriority = 'H' | 'PM' | 'PL' | 'L'

// 报警事件
export interface AlarmEvent {
  alarm_id: string
  alarm_code: string
  alarm_message: string
  priority: AlarmPriority
  event_phase: string
  alarm_time: string
  device: DeviceInfo
  vital_value: number | null
  threshold_low: number | null
  threshold_high: number | null
}

// 推送消息类型
export type PushType =
  | 'vital_signs'
  | 'waveform'
  | 'alarm'
  | 'device_status'
  | 'heartbeat'
  | 'subscribe_ack'
  | 'error'

// 推送消息
export interface PushMessage {
  msg_type: PushType
  bed_no: number
  timestamp: number
  data?: any
}

// 床位状态
export interface BedState {
  bedNo: number
  device: DeviceInfo | null
  online: boolean
  lastSeen: number
  vital: VitalSignsData | null
  waves: Record<string, WaveformData>
  alarms: AlarmEvent[]
  patientName: string
  deviceVendor: string
}

// 阈值配置
export interface ThresholdConfig {
  heartRateLow: number
  heartRateHigh: number
  pulseRateLow: number
  pulseRateHigh: number
  respRateLow: number
  respRateHigh: number
  spo2Low: number
  spo2High: number
  piLow: number
  piHigh: number
  nibpSysLow: number
  nibpSysHigh: number
  nibpDiaLow: number
  nibpDiaHigh: number
  nibpMapLow: number
  nibpMapHigh: number
  ibpSysLow: number
  ibpSysHigh: number
  ibpDiaLow: number
  ibpDiaHigh: number
  ibpMapLow: number
  ibpMapHigh: number
  temperatureLow: number
  temperatureHigh: number
}

// 显示项配置
export interface DisplayConfig {
  heartRate: boolean
  ecgWave: boolean
  resp: boolean
  spo2: boolean
  pulseRate: boolean
  perfusionIndex: boolean
  nibp: boolean
  ibp: boolean
  ibpWave: boolean
}

// 字号配置
export interface FontSizeConfig {
  heartRate: number
  ibpSys: number
  ibpDia: number
  ibpMap: number
  respRate: number
  spo2: number
  perfusionIndex: number
  pulseRate: number
  nibpSys: number
  nibpDia: number
  nibpMap: number
}

// 全局设置
export interface GlobalSettings {
  tileDisplay: DisplayConfig
  dualLeftDisplay: DisplayConfig
  dualRightDisplay: DisplayConfig
  refreshInterval: number
  defaultEcgLead: ECGLead
  thresholds: ThresholdConfig
  retentionMinutes: RetentionMinutesConfig
  fontSize: FontSizeConfig
}

// 数据保留分钟配置
export interface RetentionMinutesConfig {
  vital: number
  wave: number
  alarm: number
}

// 床位独立设置
export interface BedSettings {
  enabled: boolean
  bedEnabled: boolean
  tileDisplay: DisplayConfig
  dualLeftDisplay: DisplayConfig
  dualRightDisplay: DisplayConfig
  defaultEcgLead: ECGLead
  thresholds: ThresholdConfig
  fontSize?: FontSizeConfig
}

// 全局配置
export const WS_URL = 'ws://localhost:8080/ws'
export const MAX_BEDS = 8
export const WAVE_POINTS = 500 // 波形显示点数

// 默认显示配置（平铺模式）
export const DEFAULT_TILE_DISPLAY: DisplayConfig = {
  heartRate: true,
  ecgWave: true,
  resp: true,
  spo2: true,
  pulseRate: true,
  perfusionIndex: true,
  nibp: true,
  ibp: true,
  ibpWave: true,
}

// 默认显示配置（双栏左栏）
export const DEFAULT_DUAL_LEFT_DISPLAY: DisplayConfig = {
  heartRate: true,
  ecgWave: false,
  resp: true,
  spo2: true,
  pulseRate: true,
  perfusionIndex: true,
  nibp: true,
  ibp: true,
  ibpWave: false,
}

// 默认显示配置（双栏右栏）
export const DEFAULT_DUAL_RIGHT_DISPLAY: DisplayConfig = {
  heartRate: true,
  ecgWave: true,
  resp: true,
  spo2: true,
  pulseRate: true,
  perfusionIndex: true,
  nibp: true,
  ibp: true,
  ibpWave: true,
}

// 默认阈值配置
export const DEFAULT_THRESHOLDS: ThresholdConfig = {
  heartRateLow: 50,
  heartRateHigh: 120,
  pulseRateLow: 50,
  pulseRateHigh: 120,
  respRateLow: 10,
  respRateHigh: 30,
  spo2Low: 90,
  spo2High: 100,
  piLow: 0.1,
  piHigh: 20,
  nibpSysLow: 90,
  nibpSysHigh: 140,
  nibpDiaLow: 60,
  nibpDiaHigh: 90,
  nibpMapLow: 70,
  nibpMapHigh: 100,
  ibpSysLow: 90,
  ibpSysHigh: 140,
  ibpDiaLow: 60,
  ibpDiaHigh: 90,
  ibpMapLow: 70,
  ibpMapHigh: 100,
  temperatureLow: 35.5,
  temperatureHigh: 39,
}

// 默认字号配置
export const DEFAULT_FONT_SIZE: FontSizeConfig = {
  heartRate: 90,
  ibpSys: 40,
  ibpDia: 40,
  ibpMap: 28,
  respRate: 70,
  spo2: 45,
  perfusionIndex: 30,
  pulseRate: 30,
  nibpSys: 26,
  nibpDia: 26,
  nibpMap: 24,
}

// 默认数据保留分钟配置
export const DEFAULT_RETENTION_MINUTES: RetentionMinutesConfig = {
  vital: 90,
  wave: 2,
  alarm: 2,
}

// 默认全局设置
export const DEFAULT_GLOBAL_SETTINGS: GlobalSettings = {
  tileDisplay: { ...DEFAULT_TILE_DISPLAY },
  dualLeftDisplay: { ...DEFAULT_DUAL_LEFT_DISPLAY },
  dualRightDisplay: { ...DEFAULT_DUAL_RIGHT_DISPLAY },
  refreshInterval: 1,
  defaultEcgLead: 'II',
  thresholds: { ...DEFAULT_THRESHOLDS },
  retentionMinutes: { ...DEFAULT_RETENTION_MINUTES },
  fontSize: { ...DEFAULT_FONT_SIZE },
}
