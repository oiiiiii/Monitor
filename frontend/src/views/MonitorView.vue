<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useBedStore } from '@/stores/bed'
import { useSettingsStore } from '@/stores/settings'
import { connectWs, disconnectWs, wsConnected } from '@/services/websocket'
import BedCard from '@/components/BedCard.vue'
import GlobalSettingsModal from '@/components/GlobalSettingsModal.vue'
import type { BedState } from '@/types'

const bedStore = useBedStore()
const settingsStore = useSettingsStore()

type ViewMode = 'tile' | 'dual'

const currentTime = ref(new Date().toLocaleTimeString())
let timeInterval: ReturnType<typeof setInterval> | null = null
const viewMode = ref<ViewMode>('tile')
const showOnlyOnline = ref(false)
const leftPanelCollapsed = ref(false)
const selectedBedNo = ref<number | null>(null)
const settingsVisible = ref(false)
// 平铺模式下双击放大的床位
const focusedBedNo = ref<number | null>(null)

const highestAlarm = computed(() => bedStore.highestPriorityAlarm)

const onlineBeds = computed(() => bedStore.beds.filter(b => b.online))
const enabledBeds = computed(() => bedStore.beds.filter(b => settingsStore.getBedSettings(b.bedNo).bedEnabled))
const displayBeds = computed(() => {
  const filtered = showOnlyOnline.value ? onlineBeds.value : bedStore.beds
  return filtered.filter(b => settingsStore.getBedSettings(b.bedNo).bedEnabled)
})

const selectedBed = computed(() => {
  if (selectedBedNo.value === null) return null
  return bedStore.beds.find(b => b.bedNo === selectedBedNo.value) || null
})

const focusedBed = computed(() => {
  if (focusedBedNo.value === null) return null
  return bedStore.beds.find(b => b.bedNo === focusedBedNo.value) || null
})

function focusBed(bed: BedState) {
  focusedBedNo.value = bed.bedNo
}

function unfocusBed() {
  focusedBedNo.value = null
}

// 切换到双栏模式时退出放大视图
watch(viewMode, () => {
  focusedBedNo.value = null
})

function onGlobalKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') unfocusBed()
}

// 平铺模式网格布局计算
const tileGridStyle = computed(() => {
  const count = displayBeds.value.length
  if (!showOnlyOnline.value) {
    return {
      gridTemplateColumns: 'repeat(4, 1fr)',
      gridTemplateRows: 'repeat(2, 1fr)',
    }
  }
  // 仅显示在线时自适应
  const cols = getTileCols(count)
  const rows = Math.ceil(count / cols)
  return {
    gridTemplateColumns: `repeat(${cols}, 1fr)`,
    gridTemplateRows: `repeat(${rows}, 1fr)`,
  }
})

function getTileCols(count: number): number {
  switch (count) {
    case 0: return 1
    case 1: return 1
    case 2: return 2
    case 3: return 3
    case 4: return 2
    case 5: return 3
    case 6: return 3
    case 7: return 4
    case 8: return 4
    default: return 4
  }
}

// 双栏模式左侧网格布局计算
const dualLeftGridStyle = computed(() => {
  const count = displayBeds.value.length
  if (!showOnlyOnline.value) {
    return {
      gridTemplateColumns: 'repeat(2, 1fr)',
      gridTemplateRows: 'repeat(4, 1fr)',
    }
  }
  const cols = getDualLeftCols(count)
  const rows = Math.ceil(count / cols)
  return {
    gridTemplateColumns: `repeat(${cols}, 1fr)`,
    gridTemplateRows: `repeat(${rows}, 1fr)`,
  }
})

function getDualLeftCols(count: number): number {
  switch (count) {
    case 0: return 1
    case 1: return 1
    case 2: return 1
    case 3: return 1
    case 4: return 1
    case 5: return 1
    case 6: return 2
    case 7: return 2
    case 8: return 2
    default: return 2
  }
}

function selectBed(bed: BedState) {
  selectedBedNo.value = bed.bedNo
  if (leftPanelCollapsed.value) {
    leftPanelCollapsed.value = false
  }
}

function toggleLeftPanel() {
  leftPanelCollapsed.value = !leftPanelCollapsed.value
}

function playAlarmSound() {
  try {
    const ctx = new (window.AudioContext || (window as any).webkitAudioContext)()
    const osc = ctx.createOscillator()
    const gain = ctx.createGain()
    osc.connect(gain)
    gain.connect(ctx.destination)
    osc.frequency.value = 800
    gain.gain.setValueAtTime(0.1, ctx.currentTime)
    gain.gain.exponentialRampToValueAtTime(0.01, ctx.currentTime + 0.2)
    osc.start()
    osc.stop(ctx.currentTime + 0.2)
  } catch (e) {
    // ignore
  }
}

onMounted(() => {
  bedStore.initBeds()
  bedStore.setupMessageHandler()
  connectWs()
  setTimeout(() => {
    bedStore.subscribeAll()
  }, 500)
  timeInterval = setInterval(() => {
    currentTime.value = new Date().toLocaleTimeString()
  }, 1000)
  window.addEventListener('keydown', onGlobalKeydown)
})

onUnmounted(() => {
  disconnectWs()
  if (timeInterval) {
    clearInterval(timeInterval)
  }
  window.removeEventListener('keydown', onGlobalKeydown)
})
</script>

<template>
  <div class="monitor-app">
    <!-- 顶部状态栏 -->
    <header class="top-bar">
      <div class="title">多床位心电监护系统</div>

      <!-- 视图切换按钮 -->
      <div class="view-switch">
        <button
          :class="['view-btn', { active: viewMode === 'tile' }]"
          @click="viewMode = 'tile'"
        >
          平铺模式
        </button>
        <button
          :class="['view-btn', { active: viewMode === 'dual' }]"
          @click="viewMode = 'dual'"
        >
          双栏模式
        </button>
        <label class="online-check">
          <input type="checkbox" v-model="showOnlyOnline" />
          <span>仅显示在线设备</span>
        </label>
      </div>

      <div class="stats">
        <div class="stat-item">
          <span class="stat-label">在线:</span>
          <span class="stat-value online">{{ bedStore.onlineCount }}</span>
          <span class="stat-label">/{{ bedStore.beds.length }}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">活跃报警:</span>
          <span :class="['stat-value', bedStore.activeAlarmCount > 0 ? 'alarm' : '']">
            {{ bedStore.activeAlarmCount }}
          </span>
        </div>
        <div :class="['conn-status', wsConnected ? 'connected' : 'disconnected']">
          <span class="dot" />
          {{ wsConnected ? '已连接' : '未连接' }}
        </div>
      </div>
      <div class="header-right">
        <button class="settings-btn" @click="settingsVisible = true" title="设置">
          设置
        </button>
        <div class="time">{{ currentTime }}</div>
      </div>
    </header>

    <!-- 主内容区 -->
    <main class="main-content">
      <!-- 平铺模式 -->
      <template v-if="viewMode === 'tile'">
        <!-- 双击放大视图 -->
        <div v-if="focusedBed" class="focus-container" @dblclick="unfocusBed">
          <BedCard
            :bed="focusedBed"
            mode="dualRight"
            class="focus-card"
          />
          <div class="focus-hint">双击或按 Esc 返回平铺</div>
        </div>
        <template v-else>
          <div v-if="displayBeds.length === 0" class="empty-state">
            未接入设备
          </div>
          <div v-else class="bed-grid tile-grid" :style="tileGridStyle">
            <BedCard
              v-for="bed in displayBeds"
              :key="bed.bedNo"
              :bed="bed"
              mode="tile"
              @dblclick="focusBed(bed)"
            />
          </div>
        </template>
      </template>

      <!-- 双栏模式 -->
      <template v-else>
        <div class="dual-container">
          <!-- 左侧栏 -->
          <aside
            :class="['left-panel', { collapsed: leftPanelCollapsed }]"
          >
            <div class="left-toggle" @click="toggleLeftPanel">
              {{ leftPanelCollapsed ? '›' : '‹' }}
            </div>
            <div v-if="!leftPanelCollapsed" class="left-content">
              <div v-if="displayBeds.length === 0" class="empty-state small">
                未接入设备
              </div>
              <div v-else class="bed-grid dual-left-grid" :style="dualLeftGridStyle">
                <BedCard
                  v-for="bed in displayBeds"
                  :key="bed.bedNo"
                  :bed="bed"
                  mode="dualLeft"
                  @click="selectBed(bed)"
                  :class="{ selected: selectedBedNo === bed.bedNo }"
                />
              </div>
            </div>
          </aside>

          <!-- 右侧详情区 -->
          <section class="right-panel">
            <div v-if="!selectedBed" class="empty-state large">
              请选择左侧设备查看详情
            </div>
            <div v-else class="detail-container">
              <BedCard :bed="selectedBed" mode="dualRight" class="detail-card" />
            </div>
          </section>
        </div>
      </template>
    </main>

    <!-- 底部状态栏 -->
    <footer class="bottom-bar">
      <div v-if="highestAlarm" class="global-alarm" :class="highestAlarm.priority">
        ⚠ {{ highestAlarm.alarm_message }} - {{ highestAlarm.device.bed_no }}床
      </div>
      <div v-else class="no-alarm">系统运行正常</div>
    </footer>

    <!-- 全局设置弹窗 -->
    <GlobalSettingsModal
      :visible="settingsVisible"
      @close="settingsVisible = false"
    />
  </div>
</template>

<style scoped>
.monitor-app {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: #000;
  color: #fff;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  overflow: hidden;
}

.top-bar {
  height: 48px;
  background: linear-gradient(180deg, #1a1a2e 0%, #0f0f1a 100%);
  border-bottom: 1px solid #333;
  display: flex;
  align-items: center;
  padding: 0 20px;
  gap: 20px;
  flex-shrink: 0;
}

.title {
  font-size: 18px;
  font-weight: bold;
  color: #fff;
  letter-spacing: 2px;
  flex-shrink: 0;
}

.view-switch {
  display: flex;
  align-items: center;
  gap: 8px;
}

.view-btn {
  padding: 5px 14px;
  border: 1px solid #444;
  background: #222;
  color: #aaa;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  transition: all 0.2s;
}

.view-btn:hover {
  background: #333;
  color: #fff;
}

.view-btn.active {
  background: #2a4a6a;
  border-color: #4a8aca;
  color: #fff;
}

.online-check {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: 12px;
  font-size: 13px;
  color: #aaa;
  cursor: pointer;
  user-select: none;
}

.online-check input {
  cursor: pointer;
}

.stats {
  display: flex;
  gap: 20px;
  flex: 1;
  justify-content: center;
}

.stat-item {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 14px;
}

.stat-label {
  color: #888;
}

.stat-value {
  font-weight: bold;
  color: #fff;
}

.stat-value.online {
  color: #00ff00;
}

.stat-value.alarm {
  color: #ff4444;
}

.conn-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  padding: 4px 10px;
  border-radius: 4px;
}

.conn-status .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.conn-status.connected {
  color: #00ff00;
  background: rgba(0, 255, 0, 0.1);
}

.conn-status.connected .dot {
  background: #00ff00;
  box-shadow: 0 0 4px #00ff00;
}

.conn-status.disconnected {
  color: #ff4444;
  background: rgba(255, 68, 68, 0.1);
}

.conn-status.disconnected .dot {
  background: #ff4444;
}

.time {
  font-size: 14px;
  color: #aaa;
  font-family: monospace;
  min-width: 80px;
  text-align: right;
  flex-shrink: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}

.settings-btn {
  padding: 4px 12px;
  background: #222;
  border: 1px solid #444;
  border-radius: 4px;
  color: #ccc;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.settings-btn:hover {
  background: #333;
  color: #fff;
  border-color: #666;
}

.main-content {
  flex: 1;
  overflow: hidden;
  display: flex;
}

.bed-grid {
  display: grid;
  gap: 8px;
  padding: 8px;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.tile-grid {
  flex: 1;
}

/* 平铺模式双击放大视图 */
.focus-container {
  flex: 1;
  position: relative;
  padding: 8px;
  box-sizing: border-box;
  overflow: hidden;
  display: flex;
}

.focus-card {
  width: 100%;
  height: 100%;
}

.focus-hint {
  position: absolute;
  top: 16px;
  right: 20px;
  padding: 4px 10px;
  background: rgba(0, 0, 0, 0.6);
  border: 1px solid #444;
  border-radius: 4px;
  color: #888;
  font-size: 12px;
  pointer-events: none;
  user-select: none;
}

.empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #555;
  font-size: 20px;
}

.empty-state.small {
  font-size: 14px;
}

.empty-state.large {
  font-size: 24px;
}

/* 双栏模式 */
.dual-container {
  display: flex;
  width: 100%;
  height: 100%;
  position: relative;
}

.left-panel {
  position: relative;
  background: #0a0a0f;
  border-right: 1px solid #333;
  transition: width 0.3s ease;
  flex-shrink: 0;
  width: 40%;
}

.left-panel.collapsed {
  width: 5%;
  min-width: 50px;
}

.left-toggle {
  position: absolute;
  top: 50%;
  right: -12px;
  transform: translateY(-50%);
  width: 24px;
  height: 48px;
  background: #222;
  border: 1px solid #444;
  border-left: none;
  border-radius: 0 6px 6px 0;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: #888;
  font-size: 18px;
  z-index: 10;
  user-select: none;
}

.left-toggle:hover {
  background: #333;
  color: #fff;
}

.left-content {
  width: 100%;
  height: 100%;
  padding: 8px;
  box-sizing: border-box;
  overflow: hidden;
}

.dual-left-grid {
  width: 100%;
  height: 100%;
}

.mini-bed-card {
  background: #111;
  border: 1px solid #333;
  border-radius: 4px;
  padding: 6px 8px;
  cursor: pointer;
  transition: all 0.2s;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mini-bed-card:hover {
  border-color: #555;
  background: #1a1a1a;
}

.mini-bed-card.selected {
  border-color: #4a8aca;
  background: #1a2a3a;
}

.mini-bed-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
}

.mini-bed-no {
  font-weight: bold;
  color: #ccc;
}

.mini-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.mini-dot.online {
  background: #00ff00;
  box-shadow: 0 0 4px #00ff00;
}

.mini-dot.offline {
  background: #555;
}

.mini-vitals {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.mini-vital-row {
  display: flex;
  justify-content: center;
}

.mini-vital-value {
  font-size: 14px;
  font-weight: bold;
  font-family: 'Consolas', monospace;
}

.mini-vital-value.hr { color: #00ff00; }
.mini-vital-value.spo2 { color: #00ffff; }
.mini-vital-value.resp { color: #ffff00; }
.mini-vital-value.ibp { color: #ff0000; font-size: 12px; }
.mini-vital-value.nibp { color: #ffffff; font-size: 12px; }

.right-panel {
  flex: 1;
  padding: 8px;
  box-sizing: border-box;
  overflow: hidden;
  display: flex;
}

.detail-container {
  width: 100%;
  height: 100%;
}

.detail-card {
  width: 100%;
  height: 100%;
}

.bottom-bar {
  height: 32px;
  background: #111;
  border-top: 1px solid #333;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.global-alarm {
  font-size: 14px;
  font-weight: bold;
  padding: 4px 20px;
  border-radius: 4px;
  animation: pulse 0.8s ease-in-out infinite;
}

.global-alarm.H {
  background: #ff0000;
  color: #fff;
}

.global-alarm.PM {
  background: #ffaa00;
  color: #000;
}

.global-alarm.PL {
  background: #ffff00;
  color: #000;
}

.no-alarm {
  color: #555;
  font-size: 13px;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.7; }
}
</style>
