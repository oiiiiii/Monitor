// 床位数据 Store
import { defineStore } from 'pinia'
import { ref, computed, shallowRef, triggerRef } from 'vue'
import type { BedState, VitalSignsData, WaveformData, AlarmEvent } from '@/types'
import { onMessage, subscribeBed, unsubscribeBed } from '@/services/websocket'
import { useSettingsStore } from './settings'

const MAX_BEDS = 8
const MAX_WAVE_POINTS = 2000 // 波形显示点数（约 10 秒，200Hz）
const MAX_ALARMS = 20 // 每个床位最多保留的报警数

export const useBedStore = defineStore('bed', () => {
  // 床位状态数组（1-8 号床）使用 shallowRef 避免深度响应式开销
  const beds = shallowRef<BedState[]>([])

  // 初始化床位
  function initBeds() {
    const settingsStore = useSettingsStore()
    beds.value = []
    for (let i = 1; i <= MAX_BEDS; i++) {
      beds.value.push({
        bedNo: i,
        device: null,
        online: false,
        lastSeen: 0,
        vital: null,
        waves: {},
        alarms: [],
        patientName: settingsStore.getPatientName(i),
        deviceVendor: '',
      })
    }
  }

  // 获取指定床位（直接使用索引，避免 find 遍历）
  function getBed(bedNo: number): BedState | undefined {
    return beds.value[bedNo - 1]
  }

  // 设置患者姓名
  function setPatientName(bedNo: number, name: string) {
    const bed = getBed(bedNo)
    if (!bed) return
    bed.patientName = name
  }

  // 清除患者姓名
  function clearPatientName(bedNo: number) {
    const bed = getBed(bedNo)
    if (!bed) return
    bed.patientName = ''
    bed.vital = null
    bed.waves = {}
    bed.alarms = []
    bed.online = false
  }

  // 更新设备厂商
  function setDeviceVendor(bedNo: number, vendor: string) {
    const bed = getBed(bedNo)
    if (!bed) return
    bed.deviceVendor = vendor
  }

  // 订阅床位
  function subscribe(bedNo: number) {
    subscribeBed(bedNo)
  }

  // 取消订阅
  function unsubscribe(bedNo: number) {
    unsubscribeBed(bedNo)
  }

  // 订阅所有床位
  function subscribeAll() {
    for (let i = 1; i <= MAX_BEDS; i++) {
      subscribe(i)
    }
  }

  // 更新体征数据
  function updateVital(vital: VitalSignsData) {
    const bed = getBed(vital.bed_no)
    if (!bed) return

    // 判断 NIBP 是否有新数据（新值不为 null/undefined）
    const hasNewNibp = vital.nibp_sys !== null && vital.nibp_sys !== undefined

    if (bed.vital) {
      const prev = bed.vital
      // NIBP 保留旧值：新数据为 null/undefined 时保持旧值不变
      if (!hasNewNibp) {
        vital.nibp_sys = prev.nibp_sys
        vital.nibp_dia = prev.nibp_dia
        vital.nibp_map = prev.nibp_map
      }
    }

    bed.vital = vital
    bed.online = true
    bed.lastSeen = Date.now()
    if (vital.device && vital.device.vendor) {
      const vendorName = vital.device.vendor === 'Mindray' ? '迈瑞' :
                         vital.device.vendor === 'Comen' ? '科曼' : vital.device.vendor
      bed.deviceVendor = vendorName
    }

    const bedIndex = vital.bed_no - 1
    beds.value[bedIndex] = { ...bed }
    triggerRef(beds)
  }

  // 更新波形数据
  const LEAD_NAME_MAP: Record<string, string> = {
    'avr': 'aVR',
    'avl': 'aVL',
    'avf': 'aVF',
    'AVR': 'aVR',
    'AVL': 'aVL',
    'AVF': 'aVF',
    'spo2': 'SpO2',
    'SPO2': 'SpO2',
    'resp': 'Resp',
    'RESP': 'Resp',
    'ibp': 'IBP',
    'IBP': 'IBP',
    'ecg': 'II',
    'ECG': 'II',
    'ecgi': 'I',
    'ECGI': 'I',
    'ecgii': 'II',
    'ECGII': 'II',
    'ecgiii': 'III',
    'ECGIII': 'III',
  }

  function updateWave(wave: WaveformData) {
    const bed = getBed(wave.bed_no)
    if (!bed) return

    let lead = wave.lead
    if (LEAD_NAME_MAP[lead]) {
      lead = LEAD_NAME_MAP[lead]
    } else if (lead && lead.length > 0) {
      const normalized = lead.toLowerCase()
      if (LEAD_NAME_MAP[normalized]) {
        lead = LEAD_NAME_MAP[normalized]
      }
    }
    
    const existing = bed.waves[lead]
    let newSamples: number[]
    const updateTime = Date.now() // 用于触发重绘的标记
    const totalSamples = (existing?.totalSamples ?? 0) + wave.samples.length

    if (!existing) {
      newSamples = wave.samples.slice(-MAX_WAVE_POINTS)
      bed.waves[lead] = {
        ...wave,
        lead,
        samples: newSamples,
        updateTime,
        totalSamples,
      }
      console.log('[DEBUG] new wave added:', lead, newSamples.length)
    } else {
      newSamples = [...existing.samples, ...wave.samples]
      if (newSamples.length > MAX_WAVE_POINTS) {
        newSamples = newSamples.slice(-MAX_WAVE_POINTS)
      }
      // 完全替换对象，确保响应式更新
      bed.waves[lead] = {
        ...existing,
        samples: newSamples,
        sample_time: wave.sample_time,
        updateTime,
        totalSamples,
      }
      console.log('[DEBUG] wave updated:', lead, newSamples.length)
    }

    bed.online = true
    bed.lastSeen = Date.now()
    
    const bedIndex = wave.bed_no - 1
    beds.value[bedIndex] = { ...bed }
    triggerRef(beds)
  }

  // 更新报警
  function updateAlarm(alarm: AlarmEvent) {
    const bed = getBed(alarm.device.bed_no)
    if (!bed) return

    const isStart = alarm.event_phase === 'start' || alarm.event_phase === 'active'

    if (isStart) {
      // 报警开始，添加到列表（如果不存在）
      const exists = bed.alarms.find(a => a.alarm_id === alarm.alarm_id)
      if (!exists) {
        bed.alarms.unshift(alarm)
        // 限制数量
        if (bed.alarms.length > MAX_ALARMS) {
          bed.alarms = bed.alarms.slice(0, MAX_ALARMS)
        }
      } else {
        // 更新
        Object.assign(exists, alarm)
      }
    } else {
      // 报警结束，从列表移除
      bed.alarms = bed.alarms.filter(a => a.alarm_id !== alarm.alarm_id)
    }

    bed.online = true
    bed.lastSeen = Date.now()
    
    const bedIndex = alarm.device.bed_no - 1
    beds.value[bedIndex] = { ...bed }
    triggerRef(beds)
  }

  // 获取最高优先级的活跃报警
  const highestPriorityAlarm = computed(() => {
    let highest: AlarmEvent | null = null
    for (const bed of beds.value) {
      for (const alarm of bed.alarms) {
        if (!highest || priorityValue(alarm.priority) > priorityValue(highest.priority)) {
          highest = alarm
        }
      }
    }
    return highest
  })

  // 优先级数值（用于排序）
  function priorityValue(p: string): number {
    switch (p) {
      case 'H': return 4
      case 'PM': return 3
      case 'PL': return 2
      case 'L': return 1
      default: return 0
    }
  }

  // 在线设备数
  const onlineCount = computed(() => {
    return beds.value.filter(b => b.online).length
  })

  // 活跃报警数
  const activeAlarmCount = computed(() => {
    return beds.value.reduce((sum, bed) => sum + bed.alarms.length, 0)
  })

  // 初始化消息监听
  function setupMessageHandler() {
    onMessage((msg) => {
      switch (msg.msg_type) {
        case 'vital_signs':
          if (msg.data) {
            updateVital(msg.data as VitalSignsData)
          }
          break
        case 'waveform':
          if (msg.data && Array.isArray(msg.data)) {
            for (const wave of msg.data as WaveformData[]) {
              wave.bed_no = msg.bed_no
              updateWave(wave)
            }
          }
          break
        case 'alarm':
          if (msg.data) {
            updateAlarm(msg.data as AlarmEvent)
          }
          break
      }
    })
  }

  return {
    beds,
    initBeds,
    getBed,
    setPatientName,
    clearPatientName,
    setDeviceVendor,
    subscribe,
    unsubscribe,
    subscribeAll,
    updateVital,
    updateWave,
    updateAlarm,
    highestPriorityAlarm,
    onlineCount,
    activeAlarmCount,
    setupMessageHandler,
  }
})
