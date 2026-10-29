<script setup lang="ts">
import { ref, watch } from 'vue'
import type { BedSettings, DisplayConfig, ThresholdConfig } from '@/types'
import { ECG_LEADS, DEFAULT_GLOBAL_SETTINGS } from '@/types'
import { useSettingsStore } from '@/stores/settings'
import { useBedStore } from '@/stores/bed'

const thresholdItems: { key: string; label: string; lowKey: keyof ThresholdConfig; highKey: keyof ThresholdConfig; unit: string; step: number }[] = [
  { key: 'heartRate', label: '心率', lowKey: 'heartRateLow', highKey: 'heartRateHigh', unit: 'bpm', step: 1 },
  { key: 'pulseRate', label: '脉率', lowKey: 'pulseRateLow', highKey: 'pulseRateHigh', unit: 'bpm', step: 1 },
  { key: 'respRate', label: '呼吸', lowKey: 'respRateLow', highKey: 'respRateHigh', unit: 'rpm', step: 1 },
  { key: 'spo2', label: '血氧饱和度', lowKey: 'spo2Low', highKey: 'spo2High', unit: '%', step: 1 },
  { key: 'pi', label: '灌注指数', lowKey: 'piLow', highKey: 'piHigh', unit: '', step: 0.1 },
  { key: 'nibpSys', label: '无创收缩压', lowKey: 'nibpSysLow', highKey: 'nibpSysHigh', unit: 'mmHg', step: 1 },
  { key: 'nibpDia', label: '无创舒张压', lowKey: 'nibpDiaLow', highKey: 'nibpDiaHigh', unit: 'mmHg', step: 1 },
  { key: 'nibpMap', label: '无创平均压', lowKey: 'nibpMapLow', highKey: 'nibpMapHigh', unit: 'mmHg', step: 1 },
  { key: 'ibpSys', label: '有创收缩压', lowKey: 'ibpSysLow', highKey: 'ibpSysHigh', unit: 'mmHg', step: 1 },
  { key: 'ibpDia', label: '有创舒张压', lowKey: 'ibpDiaLow', highKey: 'ibpDiaHigh', unit: 'mmHg', step: 1 },
  { key: 'ibpMap', label: '有创平均压', lowKey: 'ibpMapLow', highKey: 'ibpMapHigh', unit: 'mmHg', step: 1 },
  { key: 'temperature', label: '体温', lowKey: 'temperatureLow', highKey: 'temperatureHigh', unit: '℃', step: 0.1 },
]

const props = defineProps<{
  visible: boolean
  bedNo: number
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const settingsStore = useSettingsStore()
const bedStore = useBedStore()

const localSettings = ref<BedSettings>({
  enabled: false,
  bedEnabled: true,
  tileDisplay: { ...settingsStore.globalSettings.tileDisplay },
  dualLeftDisplay: { ...settingsStore.globalSettings.dualLeftDisplay },
  dualRightDisplay: { ...settingsStore.globalSettings.dualRightDisplay },
  defaultEcgLead: settingsStore.globalSettings.defaultEcgLead,
  thresholds: { ...settingsStore.globalSettings.thresholds },
})

watch(() => props.visible, (val) => {
  if (val) {
    const s = settingsStore.getBedSettings(props.bedNo)
    localSettings.value = JSON.parse(JSON.stringify(s))
    if (!localSettings.value.defaultEcgLead) {
      localSettings.value.defaultEcgLead = settingsStore.globalSettings.defaultEcgLead
    }
    if (!localSettings.value.thresholds) {
      localSettings.value.thresholds = { ...settingsStore.globalSettings.thresholds }
    }
    if (!localSettings.value.enabled) {
      localSettings.value.tileDisplay = { ...settingsStore.globalSettings.tileDisplay }
      localSettings.value.dualLeftDisplay = { ...settingsStore.globalSettings.dualLeftDisplay }
      localSettings.value.dualRightDisplay = { ...settingsStore.globalSettings.dualRightDisplay }
      localSettings.value.defaultEcgLead = settingsStore.globalSettings.defaultEcgLead
      localSettings.value.thresholds = { ...settingsStore.globalSettings.thresholds }
    }
  }
})

const displayGroups = [
  { key: 'tileDisplay', label: '平铺模式显示设置' },
  { key: 'dualLeftDisplay', label: '双栏左栏显示设置' },
  { key: 'dualRightDisplay', label: '双栏右栏显示设置' },
] as const

const displayItems: { key: keyof DisplayConfig; label: string }[] = [
  { key: 'heartRate', label: '心率' },
  { key: 'ecgWave', label: '心电图波形' },
  { key: 'resp', label: '呼吸' },
  { key: 'spo2', label: '血氧' },
  { key: 'pulseRate', label: '指脉率' },
  { key: 'perfusionIndex', label: '灌注指数' },
  { key: 'nibp', label: '无创血压' },
  { key: 'ibp', label: '有创血压' },
  { key: 'ibpWave', label: '有创血压波形' },
]

function handleApply() {
  settingsStore.saveBedSettings(props.bedNo, JSON.parse(JSON.stringify(localSettings.value)))
  emit('close')
}

function handleClose() {
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="modal-overlay" @click.self="handleClose">
      <div class="modal">
        <div class="modal-header">
          <span class="modal-title">{{ bedNo }} 床 - 床位设置</span>
          <button class="close-btn" @click="handleClose">×</button>
        </div>
        <div class="modal-body">
          <!-- 应用设置开关 -->
          <div class="setting-group">
            <div class="group-title">独立设置</div>
            <div class="capsule-row">
              <div :class="['capsule-switch', { on: localSettings.enabled }]" @click="localSettings.enabled = !localSettings.enabled">
                <div class="capsule-dot" />
                <span class="capsule-label">{{ localSettings.enabled ? '已启用' : '未启用' }}</span>
              </div>
              <span class="hint">{{ localSettings.enabled ? '此床位使用独立配置' : '使用全局配置' }}</span>
            </div>
          </div>

          <!-- 三组显示设置 -->
          <div v-for="group in displayGroups" :key="group.key" class="setting-group">
            <div class="group-title">{{ group.label }}</div>
            <div class="checkbox-grid" :class="{ disabled: !localSettings.enabled }">
              <label v-for="item in displayItems" :key="item.key" class="checkbox-item">
                <input
                  type="checkbox"
                  v-model="localSettings[group.key][item.key]"
                  :disabled="!localSettings.enabled"
                />
                <span>{{ item.label }}</span>
              </label>
            </div>
          </div>

          <!-- 默认心电导联 -->
          <div class="setting-group">
            <div class="group-title">默认心电导联</div>
            <div class="lead-selector">
              <div
                v-for="lead in ECG_LEADS"
                :key="lead"
                :class="['lead-option', { active: localSettings.defaultEcgLead === lead, disabled: !localSettings.enabled || (!localSettings.tileDisplay.ecgWave && !localSettings.dualLeftDisplay.ecgWave && !localSettings.dualRightDisplay.ecgWave) }]"
                @click="localSettings.defaultEcgLead = lead"
              >
                {{ lead }}
              </div>
            </div>
            <div class="lead-hint">提示：只有启用独立设置且至少一种显示模式中启用心电图波形时，导联选择才生效</div>
          </div>

          <!-- 阈值配置 -->
          <div class="setting-group">
            <div class="group-title">生命体征阈值配置</div>
            <div :class="['threshold-grid', { disabled: !localSettings.enabled }]">
              <div v-for="item in thresholdItems" :key="item.key" class="threshold-item">
                <div class="threshold-label">{{ item.label }}</div>
                <div class="threshold-inputs">
                  <input
                    type="number"
                    :step="item.step"
                    v-model="localSettings.thresholds[item.lowKey]"
                    class="threshold-input"
                    :disabled="!localSettings.enabled"
                  />
                  <span class="threshold-sep">-</span>
                  <input
                    type="number"
                    :step="item.step"
                    v-model="localSettings.thresholds[item.highKey]"
                    class="threshold-input"
                    :disabled="!localSettings.enabled"
                  />
                  <span class="threshold-unit">{{ item.unit }}</span>
                </div>
              </div>
            </div>
            <div class="lead-hint">提示：只有启用独立设置时，阈值配置才生效</div>
          </div>
        </div>
        <div class="modal-footer">
          <div></div>
          <div class="footer-right">
            <button class="btn btn-secondary" @click="handleClose">取消</button>
            <button class="btn btn-primary" @click="handleApply">确定</button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal {
  background: #1a1a2e;
  border: 1px solid #333;
  border-radius: 8px;
  width: 560px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 20px;
  border-bottom: 1px solid #333;
}

.modal-title {
  font-size: 16px;
  font-weight: bold;
  color: #fff;
}

.close-btn {
  background: none;
  border: none;
  color: #888;
  font-size: 24px;
  cursor: pointer;
  line-height: 1;
  padding: 0 4px;
}

.close-btn:hover {
  color: #fff;
}

.modal-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px 20px;
}

.setting-group {
  margin-bottom: 18px;
}

.group-title {
  font-size: 14px;
  font-weight: bold;
  color: #aaa;
  margin-bottom: 10px;
  padding-bottom: 6px;
  border-bottom: 1px solid #2a2a3a;
}

.capsule-row {
  display: flex;
  align-items: center;
  gap: 14px;
}

.capsule-switch {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  background: #333;
  border-radius: 20px;
  cursor: pointer;
  transition: all 0.2s;
  user-select: none;
}

.capsule-switch.on {
  background: #2a6a4a;
}

.capsule-dot {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #666;
  transition: all 0.2s;
}

.capsule-switch.on .capsule-dot {
  background: #00ff00;
  box-shadow: 0 0 4px #00ff00;
}

.capsule-label {
  font-size: 13px;
  color: #ccc;
}

.capsule-switch.on .capsule-label {
  color: #fff;
}

.hint {
  font-size: 12px;
  color: #666;
}

.checkbox-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 8px 12px;
}

.checkbox-grid.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.checkbox-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: #ccc;
  cursor: pointer;
  user-select: none;
}

.checkbox-item input {
  cursor: pointer;
}

.lead-selector {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.lead-option {
  padding: 6px 12px;
  background: #222;
  border: 1px solid #444;
  border-radius: 4px;
  color: #ccc;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
  min-width: 48px;
  text-align: center;
}

.lead-option:hover:not(.disabled) {
  background: #333;
  border-color: #666;
  color: #fff;
}

.lead-option.active {
  background: #2a6a9a;
  border-color: #4a8aca;
  color: #fff;
}

.lead-option.disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.lead-hint {
  font-size: 12px;
  color: #666;
  margin-top: 8px;
}

.threshold-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 10px;
}

.threshold-grid.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.threshold-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: #1a1a2e;
  border-radius: 6px;
}

.threshold-label {
  font-size: 13px;
  color: #ccc;
  min-width: 80px;
}

.threshold-inputs {
  display: flex;
  align-items: center;
  gap: 6px;
}

.threshold-input {
  width: 50px;
  padding: 4px 6px;
  background: #111;
  border: 1px solid #444;
  border-radius: 4px;
  color: #fff;
  font-size: 12px;
  text-align: center;
}

.threshold-sep {
  color: #666;
  font-size: 13px;
}

.threshold-unit {
  font-size: 12px;
  color: #888;
  min-width: 40px;
  text-align: right;
}

.btn {
  padding: 6px 16px;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.2s;
}

.btn-primary {
  background: #2a6a9a;
  color: #fff;
  border-color: #4a8aca;
}

.btn-primary:hover {
  background: #3a7aba;
}

.btn-secondary {
  background: #333;
  color: #ccc;
  border-color: #555;
}

.btn-secondary:hover {
  background: #444;
  color: #fff;
}

.modal-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  border-top: 1px solid #333;
}

.footer-right {
  display: flex;
  gap: 10px;
}
</style>
