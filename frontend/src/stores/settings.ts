// 设置 Store
import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import type { GlobalSettings, BedSettings, DisplayConfig, ThresholdConfig, RetentionMinutesConfig, FontSizeConfig } from '@/types'
import {
  DEFAULT_GLOBAL_SETTINGS,
  DEFAULT_TILE_DISPLAY,
  DEFAULT_DUAL_LEFT_DISPLAY,
  DEFAULT_DUAL_RIGHT_DISPLAY,
  DEFAULT_THRESHOLDS,
  DEFAULT_RETENTION_MINUTES,
  DEFAULT_FONT_SIZE,
  MAX_BEDS,
  type ECGLead,
} from '@/types'

const STORAGE_KEY_GLOBAL = 'monitor_global_settings'
const STORAGE_KEY_BEDS = 'monitor_bed_settings'
const STORAGE_KEY_PATIENTS = 'monitor_patients'

function loadGlobalSettings(): GlobalSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_GLOBAL)
    if (raw) {
      const parsed = JSON.parse(raw)
      return {
        ...DEFAULT_GLOBAL_SETTINGS,
        ...parsed,
        tileDisplay: { ...DEFAULT_TILE_DISPLAY, ...(parsed.tileDisplay || {}) },
        dualLeftDisplay: { ...DEFAULT_DUAL_LEFT_DISPLAY, ...(parsed.dualLeftDisplay || {}) },
        dualRightDisplay: { ...DEFAULT_DUAL_RIGHT_DISPLAY, ...(parsed.dualRightDisplay || {}) },
        defaultEcgLead: parsed.defaultEcgLead || DEFAULT_GLOBAL_SETTINGS.defaultEcgLead,
        thresholds: { ...DEFAULT_THRESHOLDS, ...(parsed.thresholds || {}) },
        retentionMinutes: { ...DEFAULT_RETENTION_MINUTES, ...(parsed.retentionMinutes || {}) },
      }
    }
  } catch (e) {
    console.warn('加载全局设置失败', e)
  }
  return JSON.parse(JSON.stringify(DEFAULT_GLOBAL_SETTINGS))
}

function loadBedSettings(): Record<number, BedSettings> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_BEDS)
    if (raw) {
      const parsed = JSON.parse(raw)
      const result: Record<number, BedSettings> = {}
      for (let i = 1; i <= MAX_BEDS; i++) {
        if (parsed[i]) {
          result[i] = {
            enabled: false,
            bedEnabled: true,
            tileDisplay: { ...DEFAULT_TILE_DISPLAY, ...(parsed[i].tileDisplay || {}) },
            dualLeftDisplay: { ...DEFAULT_DUAL_LEFT_DISPLAY, ...(parsed[i].dualLeftDisplay || {}) },
            dualRightDisplay: { ...DEFAULT_DUAL_RIGHT_DISPLAY, ...(parsed[i].dualRightDisplay || {}) },
            defaultEcgLead: DEFAULT_GLOBAL_SETTINGS.defaultEcgLead,
            thresholds: { ...DEFAULT_THRESHOLDS, ...(parsed[i].thresholds || {}) },
            ...parsed[i],
          }
        }
      }
      return result
    }
  } catch (e) {
    console.warn('加载床位设置失败', e)
  }
  return {}
}

function loadPatientNames(): Record<number, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_PATIENTS)
    if (raw) {
      return JSON.parse(raw)
    }
  } catch (e) {
    console.warn('加载患者姓名失败', e)
  }
  return {}
}

export const useSettingsStore = defineStore('settings', () => {
  const globalSettings = ref<GlobalSettings>(loadGlobalSettings())
  const bedSettings = ref<Record<number, BedSettings>>(loadBedSettings())
  const patientNames = ref<Record<number, string>>(loadPatientNames())

  // 保存全局设置
  function saveGlobalSettings(settings: GlobalSettings) {
    globalSettings.value = settings
    localStorage.setItem(STORAGE_KEY_GLOBAL, JSON.stringify(settings))
  }

  // 重置全局设置
  function resetGlobalSettings() {
    globalSettings.value = JSON.parse(JSON.stringify(DEFAULT_GLOBAL_SETTINGS))
    localStorage.setItem(STORAGE_KEY_GLOBAL, JSON.stringify(globalSettings.value))
  }

  // 获取床位设置（没有则返回默认）
  function getBedSettings(bedNo: number): BedSettings {
    if (bedSettings.value[bedNo]) {
      return bedSettings.value[bedNo]
    }
    return {
      enabled: false,
      bedEnabled: true,
      tileDisplay: { ...globalSettings.value.tileDisplay },
      dualLeftDisplay: { ...globalSettings.value.dualLeftDisplay },
      dualRightDisplay: { ...globalSettings.value.dualRightDisplay },
      defaultEcgLead: globalSettings.value.defaultEcgLead,
      thresholds: { ...globalSettings.value.thresholds },
    }
  }

  // 获取床位阈值配置（考虑独立设置开关）
  function getBedThresholds(bedNo: number): ThresholdConfig {
    const bed = bedSettings.value[bedNo]
    if (bed?.enabled && bed.thresholds) {
      return bed.thresholds
    }
    return globalSettings.value.thresholds
  }

  // 获取床位字号配置
  function getBedFontSize(bedNo: number): FontSizeConfig {
    const bed = bedSettings.value[bedNo]
    if (bed?.enabled && bed.fontSize) {
      return bed.fontSize
    }
    return globalSettings.value.fontSize
  }

  // 保存床位设置
  function saveBedSettings(bedNo: number, settings: BedSettings) {
    bedSettings.value[bedNo] = settings
    localStorage.setItem(STORAGE_KEY_BEDS, JSON.stringify(bedSettings.value))
  }

  // 获取床位显示配置（考虑独立设置开关）
  function getBedDisplayConfig(bedNo: number, mode: 'tile' | 'dualLeft' | 'dualRight'): DisplayConfig {
    const bed = bedSettings.value[bedNo]
    if (bed?.enabled) {
      if (mode === 'tile') return bed.tileDisplay
      if (mode === 'dualLeft') return bed.dualLeftDisplay
      return bed.dualRightDisplay
    }
    if (mode === 'tile') return globalSettings.value.tileDisplay
    if (mode === 'dualLeft') return globalSettings.value.dualLeftDisplay
    return globalSettings.value.dualRightDisplay
  }

  // 获取床位默认心电导联（考虑独立设置开关）
  function getBedDefaultEcgLead(bedNo: number): ECGLead {
    const bed = bedSettings.value[bedNo]
    if (bed?.enabled && bed.defaultEcgLead) {
      return bed.defaultEcgLead
    }
    return globalSettings.value.defaultEcgLead
  }

  // 获取患者姓名
  function getPatientName(bedNo: number): string {
    return patientNames.value[bedNo] || ''
  }

  // 设置患者姓名
  function setPatientName(bedNo: number, name: string) {
    if (name.trim()) {
      patientNames.value[bedNo] = name.trim()
    } else {
      delete patientNames.value[bedNo]
    }
    localStorage.setItem(STORAGE_KEY_PATIENTS, JSON.stringify(patientNames.value))
  }

  // 解除患者
  function clearPatientName(bedNo: number) {
    delete patientNames.value[bedNo]
    localStorage.setItem(STORAGE_KEY_PATIENTS, JSON.stringify(patientNames.value))
  }

  // 自动保存
  watch(
    globalSettings,
    (val) => {
      localStorage.setItem(STORAGE_KEY_GLOBAL, JSON.stringify(val))
    },
    { deep: true }
  )

  watch(
    bedSettings,
    (val) => {
      localStorage.setItem(STORAGE_KEY_BEDS, JSON.stringify(val))
    },
    { deep: true }
  )

  watch(
    patientNames,
    (val) => {
      localStorage.setItem(STORAGE_KEY_PATIENTS, JSON.stringify(val))
    },
    { deep: true }
  )

  return {
    globalSettings,
    bedSettings,
    patientNames,
    saveGlobalSettings,
    resetGlobalSettings,
    getBedSettings,
    saveBedSettings,
    getBedDisplayConfig,
    getBedDefaultEcgLead,
    getPatientName,
    setPatientName,
    clearPatientName,
    getBedThresholds,
    getBedFontSize,
  }
})
