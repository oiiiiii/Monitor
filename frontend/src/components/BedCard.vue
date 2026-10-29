<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import type { BedState } from '@/types'
import WaveformCanvas from './WaveformCanvas.vue'
import AlarmBar from './AlarmBar.vue'
import PatientEditModal from './PatientEditModal.vue'
import BedSettingsModal from './BedSettingsModal.vue'
import { useSettingsStore } from '@/stores/settings'
import { useBedStore } from '@/stores/bed'

const props = defineProps<{
  bed: BedState
  mode?: 'tile' | 'dualLeft' | 'dualRight'
}>()

const settingsStore = useSettingsStore()
const bedStore = useBedStore()

const displayMode = computed(() => props.mode || 'tile')

const displayConfig = computed(() => {
  const modeMap = {
    tile: 'tile' as const,
    dualLeft: 'dualLeft' as const,
    dualRight: 'dualRight' as const,
  }
  return settingsStore.getBedDisplayConfig(props.bed.bedNo, modeMap[displayMode.value])
})

const defaultEcgLead = computed(() => {
  return settingsStore.getBedDefaultEcgLead(props.bed.bedNo)
})

const baseFontSize = computed(() => {
  return settingsStore.getBedFontSize(props.bed.bedNo)
})

const cardRef = ref<HTMLDivElement | null>(null)
const fontSizeScale = ref(1)
let resizeObserver: ResizeObserver | null = null

const BASE_WIDTH = 500
const BASE_HEIGHT = 400
const MIN_SCALE = 0.5
const MAX_SCALE = 2.5

function updateFontSizeScale() {
  if (!cardRef.value) return

  const rect = cardRef.value.getBoundingClientRect()
  const widthScale = rect.width / BASE_WIDTH
  const heightScale = rect.height / BASE_HEIGHT
  const scale = Math.min(widthScale, heightScale)
  fontSizeScale.value = Math.max(MIN_SCALE, Math.min(MAX_SCALE, scale))
}

const fontSize = computed(() => {
  const scale = fontSizeScale.value
  const base = baseFontSize.value
  return {
    heartRate: Math.round(base.heartRate * scale),
    ibpSys: Math.round(base.ibpSys * scale),
    ibpDia: Math.round(base.ibpDia * scale),
    ibpMap: Math.round(base.ibpMap * scale),
    respRate: Math.round(base.respRate * scale),
    spo2: Math.round(base.spo2 * scale),
    perfusionIndex: Math.round(base.perfusionIndex * scale),
    pulseRate: Math.round(base.pulseRate * scale),
    nibpSys: Math.round(base.nibpSys * scale),
    nibpDia: Math.round(base.nibpDia * scale),
    nibpMap: Math.round(base.nibpMap * scale),
  }
})

const miniFontSize = computed(() => {
  const scale = fontSizeScale.value
  const base = baseFontSize.value
  const miniScale = 0.4
  return {
    heartRate: Math.round(base.heartRate * scale * miniScale),
    ibpSys: Math.round(base.ibpSys * scale * miniScale),
    ibpDia: Math.round(base.ibpDia * scale * miniScale),
    ibpMap: Math.round(base.ibpMap * scale * miniScale),
    respRate: Math.round(base.respRate * scale * miniScale),
    spo2: Math.round(base.spo2 * scale * miniScale),
    perfusionIndex: Math.round(base.perfusionIndex * scale * miniScale),
    pulseRate: Math.round(base.pulseRate * scale * miniScale),
    nibpSys: Math.round(base.nibpSys * scale * miniScale),
    nibpDia: Math.round(base.nibpDia * scale * miniScale),
    nibpMap: Math.round(base.nibpMap * scale * miniScale),
  }
})

const patientModalVisible = ref(false)
const settingsModalVisible = ref(false)
const fullLeadMode = ref(false)

const alarmPaused = ref(false)
const alarmPauseRemaining = ref(0)
let alarmPauseTimer: ReturnType<typeof setInterval> | null = null

const isStale = ref(false)

function checkStaleness() {
  const interval = settingsStore.globalSettings.refreshInterval * 1000
  const threshold = interval * 2
  const elapsed = Date.now() - props.bed.lastSeen
  isStale.value = props.bed.online && props.bed.lastSeen > 0 && elapsed > threshold
}

watch(
  () => props.bed.lastSeen,
  () => {
    checkStaleness()
  }
)

watch(
  () => settingsStore.globalSettings.refreshInterval,
  () => {
    checkStaleness()
  }
)

const alarmStates = ref<Record<string, 'normal' | 'low' | 'high'>>({})
const alarmBlinkStates = ref<Record<string, boolean>>({})
let blinkTimers: Record<string, ReturnType<typeof setInterval>> = {}

function getThresholds() {
  return settingsStore.getBedThresholds(props.bed.bedNo)
}

function checkValue(value: number | null | undefined, low: number, high: number): 'normal' | 'low' | 'high' {
  if (value === null || value === undefined) return 'normal'
  if (value < low) return 'low'
  if (value > high) return 'high'
  return 'normal'
}

function updateAlarmStates() {
  const thresholds = getThresholds()
  const vital = props.bed.vital
  
  if (!vital) {
    alarmStates.value = {}
    return
  }
  
  alarmStates.value = {
    hr: checkValue(vital.heart_rate, thresholds.heartRateLow, thresholds.heartRateHigh),
    resp: checkValue(vital.resp_rate, thresholds.respRateLow, thresholds.respRateHigh),
    spo2: checkValue(vital.spo2, thresholds.spo2Low, thresholds.spo2High),
    pi: checkValue(vital.pi, thresholds.piLow, thresholds.piHigh),
    pr: checkValue(vital.pulse_rate, thresholds.pulseRateLow, thresholds.pulseRateHigh),
    nibp_sys: checkValue(vital.nibp_sys, thresholds.nibpSysLow, thresholds.nibpSysHigh),
    nibp_dia: checkValue(vital.nibp_dia, thresholds.nibpDiaLow, thresholds.nibpDiaHigh),
    nibp_map: checkValue(vital.nibp_map, thresholds.nibpMapLow, thresholds.nibpMapHigh),
    ibp_sys: checkValue(vital.ibp_sys, thresholds.ibpSysLow, thresholds.ibpSysHigh),
    ibp_dia: checkValue(vital.ibp_dia, thresholds.ibpDiaLow, thresholds.ibpDiaHigh),
    ibp_map: checkValue(vital.ibp_map, thresholds.ibpMapLow, thresholds.ibpMapHigh),
  }
  
  for (const key of Object.keys(alarmStates.value)) {
    const state = alarmStates.value[key]
    if (state !== 'normal' && !alarmPaused.value) {
      if (!blinkTimers[key]) {
        alarmBlinkStates.value[key] = false
        blinkTimers[key] = setInterval(() => {
          alarmBlinkStates.value[key] = !alarmBlinkStates.value[key]
        }, 500)
      }
    } else {
      if (blinkTimers[key]) {
        clearInterval(blinkTimers[key])
        delete blinkTimers[key]
        alarmBlinkStates.value[key] = false
      }
    }
  }
}

function getAlarmClass(key: string): string {
  if (alarmPaused.value) return ''
  const state = alarmStates.value[key]
  if (state === 'normal') return ''
  if (alarmBlinkStates.value[key]) {
    return state === 'low' ? 'alarm-low' : 'alarm-high'
  }
  return ''
}

function getAdaptiveFontSize(baseSize: number, value: string | null | undefined, maxDigits: number): string {
  const length = (value?.toString().length || 0)
  const ratio = Math.min(1, maxDigits / Math.max(length, 1))
  const adaptiveSize = Math.round(baseSize * ratio)
  return `${adaptiveSize}px`
}

function toggleAlarmPause() {
  if (alarmPaused.value) {
    alarmPaused.value = false
    alarmPauseRemaining.value = 0
    if (alarmPauseTimer) {
      clearInterval(alarmPauseTimer)
      alarmPauseTimer = null
    }
    updateAlarmStates()
  } else {
    alarmPaused.value = true
    alarmPauseRemaining.value = 120
    for (const key of Object.keys(blinkTimers)) {
      clearInterval(blinkTimers[key])
      delete blinkTimers[key]
    }
    alarmBlinkStates.value = {}
    alarmPauseTimer = setInterval(() => {
      alarmPauseRemaining.value -= 1
      if (alarmPauseRemaining.value <= 0) {
        alarmPaused.value = false
        if (alarmPauseTimer) {
          clearInterval(alarmPauseTimer)
          alarmPauseTimer = null
        }
        updateAlarmStates()
      }
    }, 1000)
  }
}

watch(
  () => props.bed.vital,
  () => {
    updateAlarmStates()
  },
  { deep: true, immediate: true }
)

onMounted(() => {
  checkStaleness()

  nextTick(() => {
    updateFontSizeScale()
    if (cardRef.value) {
      resizeObserver = new ResizeObserver(() => {
        updateFontSizeScale()
      })
      resizeObserver.observe(cardRef.value)
    }
  })
})

onUnmounted(() => {
  if (alarmPauseTimer) {
    clearInterval(alarmPauseTimer)
  }
  for (const key of Object.keys(blinkTimers)) {
    clearInterval(blinkTimers[key])
  }
  if (resizeObserver) {
    resizeObserver.disconnect()
  }
})

function getWaveSamples(lead: string): number[] {
  const w = props.bed.waves[lead]
  return w ? w.samples : []
}

function getWaveUpdateTime(lead: string): number {
  const w = props.bed.waves[lead]
  return w?.updateTime || 0
}

const ecgWaveData = computed(() => {
  const waves = props.bed.waves
  const lead = defaultEcgLead.value
  if (waves[lead]) return { samples: waves[lead].samples, updateTime: waves[lead].updateTime || 0, totalSamples: waves[lead].totalSamples || 0 }
  const firstKey = Object.keys(waves)[0]
  if (firstKey) return { samples: waves[firstKey].samples, updateTime: waves[firstKey].updateTime || 0, totalSamples: waves[firstKey].totalSamples || 0 }
  return { samples: [], updateTime: 0, totalSamples: 0 }
})

const ecgSamples = computed(() => ecgWaveData.value.samples)
const ecgUpdateTime = computed(() => ecgWaveData.value.updateTime)
const ecgTotalSamples = computed(() => ecgWaveData.value.totalSamples)

const ibpWaveData = computed(() => {
  const w = props.bed.waves['IBP']
  return w ? { samples: w.samples, updateTime: w.updateTime || 0, totalSamples: w.totalSamples || 0 } : { samples: [], updateTime: 0, totalSamples: 0 }
})

const ibpSamples = computed(() => ibpWaveData.value.samples)
const ibpUpdateTime = computed(() => ibpWaveData.value.updateTime)
const ibpTotalSamples = computed(() => ibpWaveData.value.totalSamples)

const hasHighAlarm = computed(() => {
  return props.bed.alarms.some(a => a.priority === 'H' || a.priority === 'PM')
})

const patientName = computed(() => {
  return props.bed.patientName || '未登记'
})

const statusDotClass = computed(() => {
  if (!props.bed.online) return 'offline'
  if (isStale.value) return 'stale'
  return 'online'
})

const ECG_LEAD_NAMES = ['I', 'II', 'III', 'aVR', 'aVL', 'aVF']

const LEAD_NAME_NORMALIZE: Record<string, string> = {
  'avr': 'aVR',
  'avl': 'aVL',
  'avf': 'aVF',
  'spo2': 'SpO2',
  'resp': 'Resp',
  'ibp': 'IBP',
}

function normalizeLeadName(name: string): string {
  return LEAD_NAME_NORMALIZE[name.toLowerCase()] || name
}

const ecgLeads = computed(() => {
  const result: string[] = []
  const seen = new Set<string>()
  for (const k of Object.keys(props.bed.waves)) {
    const normalized = normalizeLeadName(k)
    if (ECG_LEAD_NAMES.includes(normalized) && !seen.has(normalized)) {
      result.push(normalized)
      seen.add(normalized)
    }
  }
  return result.sort((a, b) => {
    const order = [...ECG_LEAD_NAMES]
    return order.indexOf(a) - order.indexOf(b)
  })
})

function fmt(v: number | null | undefined): string {
  if (v === null || v === undefined) return '--'
  return v.toFixed(0)
}

function fmt1(v: number | null | undefined): string {
  if (v === null || v === undefined) return '--'
  return v.toFixed(1)
}

function handlePatientDblClick() {
  patientModalVisible.value = true
}

function handlePatientApply(name: string) {
  bedStore.setPatientName(props.bed.bedNo, name)
  settingsStore.setPatientName(props.bed.bedNo, name)
}

function handlePatientClear() {
  bedStore.clearPatientName(props.bed.bedNo)
  settingsStore.clearPatientName(props.bed.bedNo)
}

function openBedSettings() {
  settingsModalVisible.value = true
}

function toggleFullLeadMode() {
  fullLeadMode.value = !fullLeadMode.value
}
</script>

<template>
  <div
    ref="cardRef"
    :class="[
      'bed-card',
      {
        online: bed.online,
        offline: !bed.online,
        'has-alarm': hasHighAlarm,
        'full-lead-mode': fullLeadMode,
        'dual-left': displayMode === 'dualLeft',
      }
    ]"
  >
    <div class="bed-header">
      <div class="bed-no">{{ bed.bedNo }} 床</div>
      <div class="patient" @dblclick="handlePatientDblClick">{{ patientName }}</div>
      <div v-if="bed.deviceVendor" class="vendor">{{ bed.deviceVendor }}</div>
      <template v-if="displayMode !== 'dualLeft'">
        <button 
          :class="['full-lead-btn', { active: fullLeadMode }]" 
          @click="toggleFullLeadMode" 
          title="半屏全导联"
        >
          半屏全导联
        </button>
        <button 
          :class="['alarm-pause-btn', { active: alarmPaused }]" 
          @click="toggleAlarmPause" 
          :title="alarmPaused ? '恢复报警' : '暂停报警'"
        >
          {{ alarmPaused ? alarmPauseRemaining + 's' : '暂停报警' }}
        </button>
        <button class="bed-settings-btn" @click="openBedSettings" title="设置">⚙</button>
      </template>
      <div :class="['status-dot', statusDotClass]" />
    </div>

    <template v-if="displayMode === 'dualLeft'">
      <div class="dual-left-data">
        <div v-if="displayConfig.heartRate || displayConfig.ecgWave" class="dual-left-row ecg-row">
          <div v-if="displayConfig.ecgWave" class="dual-left-wave-col">
            <WaveformCanvas
              :samples="ecgSamples"
              :update-key="ecgUpdateTime"
              :total-samples="ecgTotalSamples"
              :lead-name="defaultEcgLead"
              color="#00ff00"
            />
          </div>
          <div v-if="displayConfig.heartRate" class="dual-left-value-col">
            <div :class="['dual-left-value', 'hr', getAlarmClass('hr')]" :style="{ fontSize: fontSize.heartRate + 'px' }">{{ fmt(bed.vital?.heart_rate) }}</div>
          </div>
        </div>

        <div v-if="displayConfig.ibp || displayConfig.ibpWave" class="dual-left-row ibp-row">
          <div v-if="displayConfig.ibpWave" class="dual-left-wave-col">
            <WaveformCanvas
              :samples="ibpSamples"
              :update-key="ibpUpdateTime"
              :total-samples="ibpTotalSamples"
              lead-name="IBP"
              color="#ff0000"
            />
          </div>
          <div v-if="displayConfig.ibp" class="dual-left-value-col">
            <div class="dual-left-ibp">
              <span :class="['dual-left-value', 'ibp', getAlarmClass('ibp_sys')]" :style="{ fontSize: fontSize.ibpSys + 'px' }">{{ fmt(bed.vital?.ibp_sys) }}</span>
              <span class="dual-left-sep">/</span>
              <span :class="['dual-left-value', 'ibp', getAlarmClass('ibp_dia')]" :style="{ fontSize: fontSize.ibpDia + 'px' }">{{ fmt(bed.vital?.ibp_dia) }}</span>
            </div>
          </div>
        </div>

        <div v-if="displayConfig.resp || displayConfig.spo2 || displayConfig.nibp" class="dual-left-row vitals-row">
          <div v-if="displayConfig.resp" class="dual-left-vital-item">
            <div :class="['dual-left-value', 'resp', getAlarmClass('resp')]" :style="{ fontSize: fontSize.respRate + 'px' }">{{ fmt(bed.vital?.resp_rate) }}</div>
          </div>
          <div v-if="displayConfig.spo2" class="dual-left-vital-item">
            <div :class="['dual-left-value', 'spo2', getAlarmClass('spo2')]" :style="{ fontSize: fontSize.spo2 + 'px' }">{{ fmt(bed.vital?.spo2) }}</div>
          </div>
          <div v-if="displayConfig.nibp" class="dual-left-vital-item">
            <div class="dual-left-nibp">
              <span :class="['dual-left-value', 'nibp', getAlarmClass('nibp_sys')]" :style="{ fontSize: fontSize.nibpSys + 'px' }">{{ fmt(bed.vital?.nibp_sys) }}</span>
              <span class="dual-left-sep">/</span>
              <span :class="['dual-left-value', 'nibp', getAlarmClass('nibp_dia')]" :style="{ fontSize: fontSize.nibpDia + 'px' }">{{ fmt(bed.vital?.nibp_dia) }}</span>
            </div>
          </div>
        </div>
      </div>
    </template>

    <template v-else-if="fullLeadMode">
      <div class="full-lead-container">
        <div class="full-lead-left">
          <div
            v-for="lead in ecgLeads"
            :key="lead"
            class="lead-wave-row"
          >
            <WaveformCanvas
              :samples="bed.waves[lead]?.samples || []"
              :update-key="bed.waves[lead]?.updateTime || 0"
              :total-samples="bed.waves[lead]?.totalSamples || 0"
              :lead-name="lead"
              :color="lead === defaultEcgLead ? '#00ff00' : '#00aa00'"
              :amplitude-ratio="0.12"
            />
          </div>
          <div v-if="ecgLeads.length === 0" class="empty-lead">
            无 ECG 波形数据
          </div>
        </div>
        <div class="full-lead-right">
          <div v-if="displayConfig.heartRate" class="mini-vital-item hr">
            <div :class="['mini-value', getAlarmClass('hr')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.heartRate, fmt(bed.vital?.heart_rate), 3) }">{{ fmt(bed.vital?.heart_rate) }}</div>
          </div>
          <div v-if="displayConfig.spo2" class="mini-vital-item spo2">
            <div class="spo2-data">
              <div class="spo2-row-top">
                <div :class="['mini-value', getAlarmClass('spo2')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.spo2, fmt(bed.vital?.spo2), 3) }">{{ fmt(bed.vital?.spo2) }}</div>
              </div>
              <div class="spo2-row-bottom">
                <div v-if="displayConfig.perfusionIndex" :class="['spo2-pi', getAlarmClass('pi')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.perfusionIndex, fmt1(bed.vital?.pi), 5) }">{{ fmt1(bed.vital?.pi) }}</div>
                <div v-if="displayConfig.pulseRate" :class="['spo2-pr', getAlarmClass('pr')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.pulseRate, fmt(bed.vital?.pulse_rate), 3) }">{{ fmt(bed.vital?.pulse_rate) }}</div>
              </div>
            </div>
          </div>
          <div v-if="displayConfig.resp" class="mini-vital-item resp">
            <div :class="['mini-value', getAlarmClass('resp')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.respRate, fmt(bed.vital?.resp_rate), 3) }">{{ fmt(bed.vital?.resp_rate) }}</div>
          </div>
          <div v-if="displayConfig.ibp" class="mini-vital-item ibp">
            <div class="mini-value">
              <span :class="['ibp-val', getAlarmClass('ibp_sys')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.ibpSys, fmt(bed.vital?.ibp_sys), 3) }">{{ fmt(bed.vital?.ibp_sys) }}</span>
              <span class="ibp-sep" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.ibpSys, fmt(bed.vital?.ibp_sys), 3) }">/</span>
              <span :class="['ibp-val', getAlarmClass('ibp_dia')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.ibpDia, fmt(bed.vital?.ibp_dia), 3) }">{{ fmt(bed.vital?.ibp_dia) }}</span>
            </div>
            <div :class="['mini-sub', getAlarmClass('ibp_map')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.ibpMap, fmt(bed.vital?.ibp_map), 3) }">({{ fmt(bed.vital?.ibp_map) }})</div>
          </div>
          <div v-if="displayConfig.nibp" class="mini-vital-item nibp">
            <div class="mini-value">
              <span :class="['nibp-val', getAlarmClass('nibp_sys')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.nibpSys, fmt(bed.vital?.nibp_sys), 3) }">{{ fmt(bed.vital?.nibp_sys) }}</span>
              <span class="nibp-sep" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.nibpSys, fmt(bed.vital?.nibp_sys), 3) }">/</span>
              <span :class="['nibp-val', getAlarmClass('nibp_dia')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.nibpDia, fmt(bed.vital?.nibp_dia), 3) }">{{ fmt(bed.vital?.nibp_dia) }}</span>
            </div>
            <div :class="['mini-sub', getAlarmClass('nibp_map')]" :style="{ fontSize: getAdaptiveFontSize(miniFontSize.nibpMap, fmt(bed.vital?.nibp_map), 3) }">({{ fmt(bed.vital?.nibp_map) }})</div>
          </div>
        </div>
      </div>
    </template>

    <template v-else>
      <div class="data-area">
        <!-- 第一行：心电图波形及心率 -->
        <div v-if="displayConfig.heartRate || displayConfig.ecgWave" class="data-row ecg-row">
          <div v-if="displayConfig.ecgWave" class="wave-col">
            <WaveformCanvas
              :samples="ecgSamples"
              :update-key="ecgUpdateTime"
              :total-samples="ecgTotalSamples"
              :lead-name="defaultEcgLead"
              color="#00ff00"
              :amplitude-ratio="displayMode === 'dualRight' ? 0.15 : 0.25"
            />
          </div>
          <div v-if="displayConfig.heartRate" class="value-col hr-value-col">
            <div :class="['vital-value hr-value', getAlarmClass('hr')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.heartRate, fmt(bed.vital?.heart_rate), 3) }">{{ fmt(bed.vital?.heart_rate) }}</div>
          </div>
        </div>

        <!-- 第二行：有创血压波形及血压值 -->
        <div v-if="displayConfig.ibp || displayConfig.ibpWave" class="data-row ibp-row">
          <div v-if="displayConfig.ibpWave" class="wave-col">
            <WaveformCanvas
              :samples="ibpSamples"
              :update-key="ibpUpdateTime"
              :total-samples="ibpTotalSamples"
              lead-name="IBP"
              color="#ff0000"
              :amplitude-ratio="displayMode === 'dualRight' ? 0.15 : 0.25"
            />
          </div>
          <div v-if="displayConfig.ibp" class="value-col ibp-value-col">
            <div class="ibp-main">
              <span :class="['ibp-val', getAlarmClass('ibp_sys')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.ibpSys, fmt(bed.vital?.ibp_sys), 3) }">{{ fmt(bed.vital?.ibp_sys) }}</span>
              <span class="ibp-sep" :style="{ fontSize: getAdaptiveFontSize(fontSize.ibpSys, fmt(bed.vital?.ibp_sys), 3) }">/</span>
              <span :class="['ibp-val', getAlarmClass('ibp_dia')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.ibpDia, fmt(bed.vital?.ibp_dia), 3) }">{{ fmt(bed.vital?.ibp_dia) }}</span>
            </div>
            <div :class="['ibp-map', getAlarmClass('ibp_map')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.ibpMap, fmt(bed.vital?.ibp_map), 3) }">({{ fmt(bed.vital?.ibp_map) }})</div>
          </div>
        </div>

        <!-- 第三行：呼吸、血氧、无创血压数值区 -->
        <div v-if="displayConfig.resp || displayConfig.spo2 || displayConfig.nibp" class="data-row vitals-row">
          <div v-if="displayConfig.resp" class="vital-item resp-item">
            <div :class="['vital-num resp-value', getAlarmClass('resp')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.respRate, fmt(bed.vital?.resp_rate), 3) }">{{ fmt(bed.vital?.resp_rate) }}</div>
          </div>
          <div v-if="displayConfig.spo2" class="vital-item spo2-item">
            <div class="spo2-data">
              <div class="spo2-row-top">
                <div :class="['spo2-num', getAlarmClass('spo2')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.spo2, fmt(bed.vital?.spo2), 3) }">{{ fmt(bed.vital?.spo2) }}</div>
              </div>
              <div class="spo2-row-bottom">
                <div v-if="displayConfig.perfusionIndex" :class="['spo2-pi', getAlarmClass('pi')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.perfusionIndex, fmt1(bed.vital?.pi), 5) }">{{ fmt1(bed.vital?.pi) }}</div>
                <div v-if="displayConfig.pulseRate" :class="['spo2-pr', getAlarmClass('pr')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.pulseRate, fmt(bed.vital?.pulse_rate), 3) }">{{ fmt(bed.vital?.pulse_rate) }}</div>
              </div>
            </div>
          </div>
          <div v-if="displayConfig.nibp" class="vital-item nibp-item">
            <div class="nibp-main">
              <span :class="['nibp-val', getAlarmClass('nibp_sys')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.nibpSys, fmt(bed.vital?.nibp_sys), 3) }">{{ fmt(bed.vital?.nibp_sys) }}</span>
              <span class="nibp-sep" :style="{ fontSize: getAdaptiveFontSize(fontSize.nibpSys, fmt(bed.vital?.nibp_sys), 3) }">/</span>
              <span :class="['nibp-val', getAlarmClass('nibp_dia')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.nibpDia, fmt(bed.vital?.nibp_dia), 3) }">{{ fmt(bed.vital?.nibp_dia) }}</span>
            </div>
            <div :class="['nibp-map', getAlarmClass('nibp_map')]" :style="{ fontSize: getAdaptiveFontSize(fontSize.nibpMap, fmt(bed.vital?.nibp_map), 3) }">({{ fmt(bed.vital?.nibp_map) }})</div>
          </div>
        </div>
      </div>
    </template>

    <AlarmBar :alarms="bed.alarms" />

    <PatientEditModal
      :visible="patientModalVisible"
      :bed-no="bed.bedNo"
      :current-name="bed.patientName"
      @close="patientModalVisible = false"
      @apply="handlePatientApply"
      @clear="handlePatientClear"
    />

    <BedSettingsModal
      :visible="settingsModalVisible"
      :bed-no="bed.bedNo"
      @close="settingsModalVisible = false"
    />
  </div>
</template>

<style scoped>
.bed-card {
  background: #0a0a0a;
  border: 2px solid #333;
  border-radius: 6px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  transition: border-color 0.3s;
  position: relative;
}

.bed-card.online {
  border-color: #00aa00;
}

.bed-card.offline {
  opacity: 0.6;
}

.bed-card.has-alarm {
  border-color: #ff0000;
  animation: alarm-border 1s ease-in-out infinite;
}

@keyframes alarm-border {
  0%, 100% { border-color: #ff0000; }
  50% { border-color: #ff8800; }
}

.bed-header {
  display: flex;
  align-items: center;
  padding: 6px 10px;
  background: linear-gradient(180deg, #1a1a2e 0%, #0f0f1a 100%);
  border-bottom: 1px solid #333;
  gap: 8px;
  flex-shrink: 0;
}

.bed-no {
  font-size: 16px;
  font-weight: bold;
  color: #fff;
  flex-shrink: 0;
}

.patient {
  flex: 1;
  font-size: 13px;
  color: #aaa;
  cursor: default;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.patient:hover {
  color: #fff;
}

.vendor {
  font-size: 12px;
  color: #666;
  background: #1a1a1a;
  padding: 2px 6px;
  border-radius: 3px;
  flex-shrink: 0;
}

.full-lead-btn {
  background: #222;
  border: 1px solid #444;
  color: #888;
  font-size: 11px;
  cursor: pointer;
  padding: 3px 8px;
  border-radius: 3px;
  transition: all 0.2s;
  flex-shrink: 0;
}

.full-lead-btn:hover {
  background: #333;
  color: #fff;
}

.full-lead-btn.active {
  background: #2a6a9a;
  border-color: #4a8aca;
  color: #fff;
}

.alarm-pause-btn {
  background: #222;
  border: 1px solid #444;
  color: #888;
  font-size: 11px;
  cursor: pointer;
  padding: 3px 8px;
  border-radius: 3px;
  transition: all 0.2s;
  flex-shrink: 0;
}

.alarm-pause-btn:hover {
  background: #333;
  color: #fff;
}

.alarm-pause-btn.active {
  background: #6a2a2a;
  border-color: #ca4a4a;
  color: #ff8888;
}

.bed-settings-btn {
  background: none;
  border: none;
  color: #666;
  font-size: 16px;
  cursor: pointer;
  padding: 2px 4px;
  line-height: 1;
  flex-shrink: 0;
}

.bed-settings-btn:hover {
  color: #fff;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot.online {
  background: #00ff00;
  box-shadow: 0 0 6px #00ff00;
}

.status-dot.offline {
  background: #666;
}

.status-dot.stale {
  background: #ff0000;
  box-shadow: 0 0 6px #ff0000;
  animation: stale-pulse 1s ease-in-out infinite;
}

@keyframes stale-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.alarm-low {
  color: #000000 !important;
  background-color: #ffff00 !important;
  padding: 2px 6px;
  border-radius: 4px;
}

.alarm-high {
  color: #ffffff !important;
  background-color: #ff0000 !important;
  padding: 2px 6px;
  border-radius: 4px;
}

.ibp-val, .nibp-val {
  display: inline-block;
}

.ibp-sep, .nibp-sep {
  margin: 0 2px;
}

.data-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.data-row {
  flex: 1;
  display: flex;
  min-height: 0;
  border-bottom: 1px solid #1a1a1a;
}

.data-row:last-child {
  border-bottom: none;
}

.wave-col {
  width: 60%;
  flex-shrink: 0;
  position: relative;
}

.value-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  border-left: 1px solid #1a1a1a;
  padding: 4px;
}

.vital-value {
  font-size: 90px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  line-height: 1.1;
}

.hr-value { color: #00ff00; }

/* IBP 数值区 */
.ibp-value-col {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 6px 8px;
}

.ibp-main {
  font-size: 40px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  color: #ff0000;
  text-align: center;
}

.ibp-map {
  font-size: 40px;
  color: #ff0000;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  margin-top: 2px;
}

/* 第三行数值区样式 */
.vitals-row {
  display: flex;
  align-items: center;
  background: rgba(0, 0, 0, 0.4);
  padding: 10px 0;
  gap: 8px;
}

.vital-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 4px 8px;
  border-right: 1px solid #2a2a2a;
}

.vital-item:last-child {
  border-right: none;
}

.vital-item.resp-item { flex: 25; }
.vital-item.spo2-item { flex: 30; }
.vital-item.nibp-item { flex: 45; }

.vital-num {
  font-size: 70px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
}

.resp-item .resp-value { color: #ffff00; }

.spo2-item .spo2-data {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  align-items: center;
  justify-content: center;
  gap: 2px;
}

.spo2-item .spo2-row-top {
  display: flex;
  align-items: center;
  justify-content: center;
}

.spo2-item .spo2-num {
  font-size: 45px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  color: #00ffff;
}

.spo2-item .spo2-row-bottom {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
}

.spo2-item .spo2-pi,
.spo2-item .spo2-pr {
  font-size: 30px;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  color: #00ffff;
}

.nibp-item .nibp-main {
  font-size: 40px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  color: #ffffff;
}

.nibp-item .nibp-map {
  font-size: 40px;
  color: #ffffff;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  margin-top: 2px;
}

.full-lead-container {
  flex: 1;
  display: flex;
  overflow: hidden;
}

.full-lead-left {
  width: 85%;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.lead-wave-row {
  flex: 1;
  min-height: 0;
  border-bottom: 1px solid #1a1a1a;
}

.lead-wave-row:last-child {
  border-bottom: none;
}

.empty-lead {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #555;
  font-size: 14px;
}

.full-lead-right {
  width: 20%;
  display: flex;
  flex-direction: column;
  background: rgba(0, 0, 0, 0.6);
  border-left: 1px solid #333;
}

.mini-vital-item {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 4px 2px;
  border-bottom: 1px solid #1a1a1a;
  min-height: 0;
}

.mini-vital-item:last-child {
  border-bottom: none;
}

.mini-vital-item .mini-value {
  font-size: 18px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  white-space: nowrap;
}

.mini-vital-item .mini-sub {
  font-size: 10px;
  color: #888;
  margin-top: 1px;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
}

.mini-vital-item.hr .mini-value { color: #00ff00; }
.mini-vital-item.spo2 { padding: 4px; }
.mini-vital-item.spo2 .spo2-data {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  align-items: center;
  justify-content: center;
  gap: 2px;
}
.mini-vital-item.spo2 .spo2-row-top {
  display: flex;
  align-items: center;
  justify-content: center;
}
.mini-vital-item.spo2 .spo2-row-bottom {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}
.mini-vital-item.spo2 .spo2-pi,
.mini-vital-item.spo2 .spo2-pr {
  font-size: 14px;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  color: #00ffff;
}
.mini-vital-item.resp .mini-value { color: #ffff00; }
.mini-vital-item.ibp .mini-value { color: #ff0000; }
.mini-vital-item.ibp .mini-sub { color: #ff6666; }
.mini-vital-item.nibp .mini-value { color: #ffffff; }
.mini-vital-item.nibp .mini-sub { color: #ffffff; }

.bed-card.dual-left {
  border-radius: 4px;
  min-height: 0;
}

.bed-card.dual-left .bed-header {
  padding: 4px 6px;
  gap: 4px;
}

.bed-card.dual-left .bed-no {
  font-size: 12px;
}

.bed-card.dual-left .patient {
  font-size: 11px;
}

.bed-card.dual-left .vendor {
  font-size: 10px;
  padding: 1px 4px;
}

.bed-card.dual-left .status-dot {
  width: 6px;
  height: 6px;
}

.bed-card.dual-left .dual-left-data {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.bed-card.dual-left .dual-left-row {
  flex: 1;
  display: flex;
  min-height: 0;
  border-bottom: 1px solid #1a1a1a;
}

.bed-card.dual-left .dual-left-row:last-child {
  border-bottom: none;
}

.bed-card.dual-left .dual-left-wave-col {
  width: 60%;
  flex-shrink: 0;
  min-height: 0;
}

.bed-card.dual-left .dual-left-value-col {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
  border-left: 1px solid #1a1a1a;
  padding: 2px;
}

.bed-card.dual-left .dual-left-vitals-row {
  display: flex;
  align-items: center;
  justify-content: space-around;
  background: rgba(0, 0, 0, 0.4);
  padding: 4px 0;
  gap: 4px;
}

.bed-card.dual-left .dual-left-vital-item {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 30px;
  padding: 2px 4px;
  border-right: 1px solid #2a2a2a;
}

.bed-card.dual-left .dual-left-vital-item:last-child {
  border-right: none;
}

.bed-card.dual-left .dual-left-value {
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  white-space: nowrap;
}

.bed-card.dual-left .dual-left-value.hr { color: #00ff00; }
.bed-card.dual-left .dual-left-value.spo2 { color: #00ffff; }
.bed-card.dual-left .dual-left-value.resp { color: #ffff00; }
.bed-card.dual-left .dual-left-value.ibp { color: #ff0000; }
.bed-card.dual-left .dual-left-value.nibp { color: #ffffff; }

.bed-card.dual-left .dual-left-sep {
  margin: 0 2px;
  color: inherit;
}

.bed-card.dual-left .dual-left-ibp,
.bed-card.dual-left .dual-left-nibp {
  display: flex;
  align-items: center;
}

.bed-card.dual-left.selected {
  border-color: #00aaff;
  box-shadow: 0 0 8px rgba(0, 170, 255, 0.5);
}
</style>
