<script setup lang="ts">
import { computed } from 'vue'
import type { AlarmEvent } from '@/types'

const props = defineProps<{
  alarms: AlarmEvent[]
}>()

// 按优先级排序：H > PM > PL > L
const sortedAlarms = computed(() => {
  const priorityOrder: Record<string, number> = {
    H: 4,
    PM: 3,
    PL: 2,
    L: 1,
  }
  return [...props.alarms].sort((a, b) => {
    return (priorityOrder[b.priority] || 0) - (priorityOrder[a.priority] || 0)
  })
})

function priorityClass(p: string): string {
  switch (p) {
    case 'H': return 'alarm-high'
    case 'PM': return 'alarm-medium'
    case 'PL': return 'alarm-low'
    default: return 'alarm-info'
  }
}

function priorityLabel(p: string): string {
  switch (p) {
    case 'H': return '危急'
    case 'PM': return '中'
    case 'PL': return '低'
    default: return '信息'
  }
}

// 是否正在闪烁（高优先级报警）
function isBlinking(p: string): boolean {
  return p === 'H' || p === 'PM'
}
</script>

<template>
  <div class="alarm-bar">
    <div v-if="sortedAlarms.length === 0" class="no-alarm">
      无活跃报警
    </div>
    <div
      v-for="alarm in sortedAlarms.slice(0, 3)"
      :key="alarm.alarm_id"
      :class="['alarm-item', priorityClass(alarm.priority), { blinking: isBlinking(alarm.priority) }]"
    >
      <span class="alarm-prio">[{{ priorityLabel(alarm.priority) }}]</span>
      <span class="alarm-msg">{{ alarm.alarm_message }}</span>
    </div>
  </div>
</template>

<style scoped>
.alarm-bar {
  height: 28px;
  background: #111;
  border-top: 1px solid #333;
  display: flex;
  align-items: center;
  padding: 0 8px;
  gap: 12px;
  overflow: hidden;
}

.no-alarm {
  color: #555;
  font-size: 12px;
}

.alarm-item {
  font-size: 12px;
  font-weight: bold;
  white-space: nowrap;
  padding: 2px 8px;
  border-radius: 3px;
}

.alarm-prio {
  margin-right: 4px;
}

.alarm-high {
  background: #ff0000;
  color: #fff;
}

.alarm-medium {
  background: #ffaa00;
  color: #000;
}

.alarm-low {
  background: #ffff00;
  color: #000;
}

.alarm-info {
  background: #00aaff;
  color: #fff;
}

.blinking {
  animation: blink 0.5s ease-in-out infinite;
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
</style>
