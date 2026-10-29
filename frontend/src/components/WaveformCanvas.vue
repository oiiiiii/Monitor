<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'

const props = defineProps<{
  samples: number[]
  totalSamples?: number
  color?: string
  leadName?: string
  speed?: number
  gain?: number
  updateKey?: number
  amplitudeRatio?: number
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
let animationId: number | null = null
let resizeObserver: ResizeObserver | null = null

const DPR = window.devicePixelRatio || 1
const POINTS_PER_PIXEL = 2
const DISPLAY_POINTS = ref(0)

let canvasWidth = 0
let canvasHeight = 0

function resizeCanvas() {
  const canvas = canvasRef.value
  if (!canvas) return

  const rect = canvas.getBoundingClientRect()
  if (rect.width === 0 || rect.height === 0) return

  canvas.width = Math.floor(rect.width * DPR)
  canvas.height = Math.floor(rect.height * DPR)

  canvasWidth = canvas.width
  canvasHeight = canvas.height

  DISPLAY_POINTS.value = Math.floor(canvas.width * POINTS_PER_PIXEL)

  draw()
}

function draw() {
  const canvas = canvasRef.value
  if (!canvas) return

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const w = canvasWidth
  const h = canvasHeight

  if (w === 0 || h === 0) return

  ctx.fillStyle = '#0a0a0a'
  ctx.fillRect(0, 0, w, h)

  if (props.leadName) {
    ctx.fillStyle = props.color || '#00ff00'
    ctx.font = `bold ${Math.max(12, Math.floor(h * 0.08))}px monospace`
    ctx.fillText(props.leadName, 8 * DPR, Math.max(14, Math.floor(h * 0.1)))
  }

  const samples = props.samples
  if (samples.length < 2) return

  const window_ = DISPLAY_POINTS.value
  if (window_ < 2) return

  // 扫掠式绘制：X 轴为固定时间轴 [0, window_)
  // 每个采样点按绝对序号取模定位，写指针绕回后原位覆盖旧数据
  const total = props.totalSamples ?? samples.length
  const count = Math.min(samples.length, window_)
  const view = samples.slice(-count)
  const startAbs = total - count // view[0] 的绝对序号
  const cursor = total % window_ // 写指针（下一个写入位置）
  const GAP_POINTS = 10 // 指针前方的擦除间隙，让新旧分界清晰可见

  let minVal = view[0]
  let maxVal = view[0]
  for (const val of view) {
    if (val < minVal) minVal = val
    if (val > maxVal) maxVal = val
  }
  const range = maxVal - minVal || 1

  const midY = h / 2
  const amplitude = h * (props.amplitudeRatio || 0.25)

  ctx.strokeStyle = props.color || '#00ff00'
  ctx.lineWidth = 1.5 * DPR
  ctx.beginPath()

  let prevPos = -1

  for (let i = 0; i < count; i++) {
    const pos = (startAbs + i) % window_

    // 擦除间隙：指针前方 GAP_POINTS 内的旧数据不绘制（断笔形成空隙）
    const ahead = (pos - cursor + window_) % window_
    if (ahead < GAP_POINTS) {
      prevPos = -1
      continue
    }

    const x = (pos / POINTS_PER_PIXEL) * DPR
    const val = view[i] || 0
    const normalized = ((val - minVal) / range) * 2 - 1
    const y = midY - normalized * amplitude

    // 绕回点（pos 变小）或间隙后的第一点都断笔重起，避免跨越接缝的连线
    if (prevPos < 0 || pos < prevPos) {
      ctx.moveTo(x, y)
    } else {
      ctx.lineTo(x, y)
    }
    prevPos = pos
  }

  ctx.stroke()
}

function scheduleDraw() {
  if (animationId) return
  animationId = requestAnimationFrame(() => {
    animationId = null
    draw()
  })
}

// 监听 updateKey 变化触发重绘（解决数据长度不变时卡住的问题）
watch(
  () => props.updateKey,
  () => {
    scheduleDraw()
  }
)

// 监听 samples 变化（兼容旧的调用方式）
watch(
  () => props.samples.length,
  () => {
    scheduleDraw()
  }
)

onMounted(() => {
  nextTick(() => {
    resizeCanvas()
    scheduleDraw()
  })

  resizeObserver = new ResizeObserver(() => {
    resizeCanvas()
  })

  const canvas = canvasRef.value
  if (canvas) {
    resizeObserver.observe(canvas)
  }

  window.addEventListener('resize', resizeCanvas)
})

onUnmounted(() => {
  if (animationId) {
    cancelAnimationFrame(animationId)
  }
  if (resizeObserver) {
    resizeObserver.disconnect()
  }
  window.removeEventListener('resize', resizeCanvas)
})
</script>

<template>
  <canvas
    ref="canvasRef"
    class="wave-canvas"
  />
</template>

<style scoped>
.wave-canvas {
  width: 100%;
  height: 100%;
  display: block;
  background: #0a0a0a;
}
</style>