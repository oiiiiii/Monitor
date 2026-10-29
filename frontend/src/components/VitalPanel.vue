<script setup lang="ts">
import { computed } from 'vue'
import type { VitalSignsData, DisplayConfig } from '@/types'

const props = defineProps<{
  vital: VitalSignsData | null
  displayConfig?: DisplayConfig
}>()

const config = computed(() => props.displayConfig || {
  heartRate: true,
  ecgWave: true,
  resp: true,
  spo2: true,
  pulseRate: true,
  perfusionIndex: true,
  nibp: true,
  ibp: true,
  ibpWave: true,
})

function fmt(v: number | null | undefined, unit?: string): string {
  if (v === null || v === undefined) return '--'
  return v.toFixed(0) + (unit || '')
}

const visibleItems = computed(() => {
  const items: string[] = []
  if (config.value.heartRate) items.push('hr')
  if (config.value.spo2) items.push('spo2')
  if (config.value.pulseRate) items.push('pr')
  if (config.value.resp) items.push('resp')
  if (config.value.nibp) items.push('nibp')
  return items
})

const gridCols = computed(() => visibleItems.value.length)
</script>

<template>
  <div class="vital-panel" :style="{ gridTemplateColumns: `repeat(${gridCols}, 1fr)` }">
    <!-- 心率 -->
    <div v-if="config.heartRate" class="vital-item hr">
      <div class="vital-label">HR</div>
      <div class="vital-value">{{ fmt(vital?.heart_rate) }}</div>
      <div class="vital-unit">bpm</div>
    </div>

    <!-- 血氧 -->
    <div v-if="config.spo2" class="vital-item spo2">
      <div class="vital-label">SpO2</div>
      <div class="vital-value">{{ fmt(vital?.spo2) }}</div>
      <div class="vital-unit">%</div>
    </div>

    <!-- 脉率 -->
    <div v-if="config.pulseRate" class="vital-item pr">
      <div class="vital-label">PR</div>
      <div class="vital-value">{{ fmt(vital?.pulse_rate) }}</div>
      <div class="vital-unit">bpm</div>
    </div>

    <!-- 呼吸 -->
    <div v-if="config.resp" class="vital-item resp">
      <div class="vital-label">RESP</div>
      <div class="vital-value">{{ fmt(vital?.resp_rate) }}</div>
      <div class="vital-unit">rpm</div>
    </div>

    <!-- 血压 -->
    <div v-if="config.nibp" class="vital-item nibp">
      <div class="vital-label">NIBP</div>
      <div class="vital-value bp">
        {{ fmt(vital?.nibp_sys) }}/{{ fmt(vital?.nibp_dia) }}
      </div>
      <div class="vital-unit">mmHg</div>
    </div>
  </div>
</template>

<style scoped>
.vital-panel {
  display: grid;
  gap: 4px;
  padding: 6px;
  background: rgba(0, 0, 0, 0.6);
  border-top: 1px solid #333;
}

.vital-item {
  text-align: center;
  padding: 4px 2px;
}

.vital-label {
  font-size: 11px;
  color: #888;
  font-weight: bold;
  margin-bottom: 2px;
}

.vital-value {
  font-size: 22px;
  font-weight: bold;
  font-family: 'Arial Black', 'Microsoft YaHei', Arial, sans-serif;
  line-height: 1;
}

.vital-unit {
  font-size: 10px;
  color: #666;
  margin-top: 2px;
}

.hr .vital-value {
  color: #00ff00;
}

.spo2 .vital-value {
  color: #00ffff;
}

.resp .vital-value {
  color: #ffff00;
}

.nibp .vital-value {
  color: #ffffff;
  font-size: 18px;
}

.hr .vital-label {
  color: #00aa00;
}

.hr .vital-unit {
  color: #006600;
}

.spo2 .vital-label {
  color: #00aaaa;
}

.spo2 .vital-unit {
  color: #006666;
}

.pr .vital-value {
  color: #00ffff;
}

.pr .vital-label {
  color: #00aaaa;
}

.pr .vital-unit {
  color: #006666;
}

.resp .vital-label {
  color: #aaaa00;
}

.resp .vital-unit {
  color: #666600;
}

.nibp .vital-label {
  color: #cccccc;
}

.nibp .vital-unit {
  color: #888888;
}

.bp {
  font-size: 18px !important;
}
</style>
