# Web 界面波形绘制技术实现方案

## 1. 整体架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                        后端服务 (Rust)                          │
│          接收医疗设备数据 → 解析 → 通过WebSocket推送              │
└──────────────────────────────┬──────────────────────────────────┘
                               │ WebSocket JSON消息
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                     前端数据层 (Pinia Store)                     │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ websocket.ts │───▶│   bed.ts     │───▶│ 组件订阅更新  │       │
│  │ 消息接收/分发 │    │ 数据存储/管理 │    │              │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
└──────────────────────────────┬──────────────────────────────────┘
                               │ 响应式数据流
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                     前端视图层 (Vue Components)                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                   BedCard.vue                            │    │
│  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐    │    │
│  │  │ Waveform    │   │ Waveform    │   │ 生命体征    │    │    │
│  │  │ Canvas(ECG) │   │ Canvas(IBP) │   │ 数值显示    │    │    │
│  │  └─────────────┘   └─────────────┘   └─────────────┘    │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 数据流转机制

### 2.1 WebSocket 连接管理

**文件**: [services/websocket.ts](file:///d:/Code/Monitor/frontend/src/services/websocket.ts)

| 特性 | 实现方式 |
|------|----------|
| 连接地址 | 根据环境变量或当前页面地址自动推导 (`ws://host:8080`) |
| 重连策略 | 指数退避 + 抖动 (`1s, 2s, 4s...` 最大 30s) |
| 心跳机制 | 每 25 秒发送 `{ action: 'ping' }` 保持连接 |
| 消息分发 | 观察者模式，支持多个 handler 订阅 |
| 床位订阅 | 支持动态订阅/取消订阅指定床位 |

### 2.2 波形数据存储

**文件**: [stores/bed.ts](file:///d:/Code/Monitor/frontend/src/stores/bed.ts#L9)

```typescript
const MAX_WAVE_POINTS = 600 // 波形显示点数（约 3 秒 @ 200Hz）
```

**核心设计**:

1. **固定长度循环缓冲**：波形数组最大 600 个采样点，超出时自动丢弃最旧数据
2. **shallowRef 优化**：使用 `shallowRef` 避免深度响应式开销，通过 `triggerRef` 手动触发更新
3. **导联名称标准化**：统一映射导联名称（如 `avr` → `aVR`, `spo2` → `SpO2`）

**数据更新逻辑** ([stores/bed.ts#L143-L179](file:///d:/Code/Monitor/frontend/src/stores/bed.ts#L143)):

```typescript
function updateWave(wave: WaveformData) {
  const existing = bed.waves[lead]
  if (!existing) {
    // 新导联，直接存入（限制长度）
    const samples = wave.samples.slice(-MAX_WAVE_POINTS)
    bed.waves[lead] = { ...wave, samples }
  } else {
    // 追加新样本，保持固定长度（滚动）
    const newSamples = [...existing.samples, ...wave.samples]
    if (newSamples.length > MAX_WAVE_POINTS) {
      existing.samples = newSamples.slice(-MAX_WAVE_POINTS)
    } else {
      existing.samples = newSamples
    }
  }
  triggerRef(beds) // 手动触发响应式更新
}
```

---

## 3. 波形绘制核心组件

### 3.1 WaveformCanvas 组件

**文件**: [components/WaveformCanvas.vue](file:///d:/Code/Monitor/frontend/src/components/WaveformCanvas.vue)

**组件属性**:

| 属性 | 类型 | 说明 |
|------|------|------|
| `samples` | `number[]` | 波形采样数据数组 |
| `color` | `string` | 波形颜色（默认绿色 `#00ff00`） |
| `leadName` | `string` | 导联名称（如 `II`, `IBP`） |
| `speed` | `number` | 绘制速度（预留参数） |
| `gain` | `number` | 增益系数（预留参数） |

### 3.2 绘制流程

```
数据更新 → watch 触发 → requestAnimationFrame → 绘制帧
                                                    │
                    ┌───────────────────────────────┼───────────────────────────────┐
                    ▼                               ▼                               ▼
            清空画布背景                       绘制网格线                       绘制波形曲线
            (fillRect)                      (drawGrid)                      (beginPath/lineTo)
```

### 3.3 关键技术实现

#### 3.3.1 Canvas 尺寸适配

```typescript
const POINTS_PER_PIXEL = 2  // 每像素显示的采样点数

function resizeCanvas() {
  const rect = canvas.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  
  // 处理高分辨率屏幕
  canvas.width = Math.floor(rect.width * dpr)
  canvas.height = Math.floor(rect.height * dpr)
  
  // 动态计算网格大小（根据画布尺寸）
  gridSize.value = Math.floor(Math.min(rect.width, rect.height) / 50) || 10
}
```

**DPR 适配原理**:
- 高分辨率屏幕（如 Retina）的 `devicePixelRatio` 通常为 2 或更高
- Canvas 实际像素尺寸 = CSS 尺寸 × DPR，确保绘制清晰不模糊

#### 3.3.2 网格绘制

```typescript
function drawGrid(ctx: CanvasRenderingContext2D, w: number, h: number) {
  const smallGrid = gridSize.value      // 小网格间距（约 10px）
  const bigGrid = smallGrid * 5         // 大网格间距（5个小网格）
  
  // 小网格（浅灰色，低透明度）
  ctx.strokeStyle = 'rgba(80, 80, 80, 0.15)'
  ctx.lineWidth = 0.5
  // ... 绘制水平和垂直线
  
  // 大网格（稍深灰色，稍高透明度）
  ctx.strokeStyle = 'rgba(100, 100, 100, 0.2)'
  ctx.lineWidth = 1
  // ... 绘制水平和垂直线
}
```

**视觉效果**:
- 深色背景 (`#0a0a0a`) 配合灰色网格，符合医疗监护仪风格
- 两级网格提供尺度参考

#### 3.3.3 波形绘制算法

**核心步骤**:

1. **计算显示窗口**：

```typescript
const maxPoints = w * POINTS_PER_PIXEL           // 画布可容纳的最大点数
const startIndex = Math.max(0, samples.length - maxPoints)  // 起始索引
const endIndex = samples.length                  // 结束索引（最新数据）
```

2. **动态范围自适应**：

```typescript
// 计算当前显示窗口内数据的最小值和最大值
let minVal = samples[startIndex]
let maxVal = samples[startIndex]
for (let i = startIndex; i < endIndex; i++) {
  const val = samples[i]
  if (val < minVal) minVal = val
  if (val > maxVal) maxVal = val
}
const range = maxVal - minVal || 1  // 防止除零
```

3. **坐标映射与绘制**：

```typescript
const midY = h / 2                  // 垂直中心线
const amplitude = h * 0.35          // 波形振幅（占画布高度的 70%）

ctx.strokeStyle = props.color || '#00ff00'
ctx.lineWidth = 1.5
ctx.beginPath()

for (let i = startIndex; i < endIndex; i++) {
  // X轴：从右向左绘制，新数据在右侧
  const x = (i - startIndex) / POINTS_PER_PIXEL
  
  // Y轴：归一化后映射到画布
  const val = samples[i] || 0
  const normalized = ((val - minVal) / range) * 2 - 1  // [-1, 1]
  const y = midY - normalized * amplitude              // 翻转Y轴
  
  if (!started) {
    ctx.moveTo(x, y)
    started = true
  } else {
    ctx.lineTo(x, y)
  }
}
ctx.stroke()
```

**算法特点**:

| 特性 | 实现方式 |
|------|----------|
| 绘制方向 | 从右向左，新数据追加在右侧，旧数据向左滚动 |
| Y轴翻转 | 心电图惯例：向上为正，向下为负 |
| 自适应缩放 | 根据当前窗口数据动态调整幅度，确保波形充满画布 |
| 空值处理 | `samples[i] || 0` 防止 NaN 导致绘制异常 |

#### 3.3.4 性能优化

```typescript
let animationId: number | null = null

function scheduleDraw() {
  if (animationId) return  // 防抖：避免重复调度
  animationId = requestAnimationFrame(() => {
    animationId = null
    draw()
  })
}

// 仅监听数组长度变化，避免深度比较
watch(
  () => props.samples.length,
  (newLen) => {
    if (newLen !== lastSamplesLength.value) {
      lastSamplesLength.value = newLen
      scheduleDraw()
    }
  }
)
```

**优化策略**:

1. **requestAnimationFrame**：与浏览器渲染帧同步，避免不必要的绘制
2. **防抖机制**：同一帧内多次数据更新只触发一次绘制
3. **长度监听**：仅监听 `samples.length` 变化，而非整个数组的深度监听

---

## 4. 波形显示配置

### 4.1 颜色规范

| 波形类型 | 颜色 | 颜色代码 |
|----------|------|----------|
| ECG (心电图) | 绿色 | `#00ff00` |
| SpO₂ (血氧) | 青色 | `#00ffff` |
| RESP (呼吸) | 黄色 | `#ffff00` |
| IBP (有创血压) | 红色 | `#ff0000` |

### 4.2 显示控制

**文件**: [types/index.ts](file:///d:/Code/Monitor/frontend/src/types/index.ts#L131)

```typescript
export interface DisplayConfig {
  heartRate: boolean      // 心率数值
  ecgWave: boolean        // 心电图波形
  resp: boolean           // 呼吸率
  spo2: boolean           // 血氧
  pulseRate: boolean      // 脉率
  perfusionIndex: boolean // 灌注指数
  nibp: boolean           // 无创血压
  ibp: boolean            // 有创血压数值
  ibpWave: boolean        // 有创血压波形
}
```

---

## 5. 技术选型考量

### 5.1 为什么选择 Canvas API 而非 SVG/ECharts

| 对比维度 | Canvas | SVG | ECharts |
|----------|--------|-----|---------|
| 性能 | 高（直接像素操作） | 低（DOM元素） | 中（封装Canvas） |
| 实时性 | 最佳（60fps） | 差（重排开销） | 较好 |
| 灵活性 | 完全可控 | 中等 | 受限于配置 |
| 代码量 | 中等 | 较少 | 最少 |
| 适用场景 | 高频实时波形 | 静态图表 | 数据分析图表 |

**决策依据**:
- 医疗波形需要 200Hz 采样率，每秒更新多次，Canvas 是唯一可行方案
- 全量重绘方式避免增量绘制导致的图像质量下降（经验教训：`drawImage()` 会导致颜色渐变和模糊）

### 5.2 全量重绘 vs 增量绘制

**历史经验教训**（项目记忆）:

> Incremental drawing with drawImage() causes image quality degradation and color fading over time

**当前方案**：每次绘制时先清空画布（`fillRect`），然后重绘网格和波形。

**优点**:
- 避免图像质量退化
- 实现简单，逻辑清晰
- 配合 `requestAnimationFrame` 性能可接受

---

## 6. 数据格式

### 6.1 WaveformData 类型

```typescript
export interface WaveformData {
  bed_no: number        // 床号
  device_id: string     // 设备ID
  lead: string          // 导联名称
  sample_time: string   // 采样时间
  sample_rate: number   // 采样率（Hz）
  samples: number[]     // 采样值数组
}
```

### 6.2 WebSocket 消息格式

```json
{
  "msg_type": "waveform",
  "bed_no": 1,
  "timestamp": 1620000000000,
  "data": {
    "device_id": "dev001",
    "lead": "II",
    "sample_time": "2026-06-29T10:00:00.000Z",
    "sample_rate": 200,
    "samples": [0.1, 0.2, 0.3, ...]
  }
}
```

---

## 7. 边界情况处理

| 边界情况 | 处理方式 |
|----------|----------|
| 数据不足（< 2个点） | 跳过绘制，等待更多数据 |
| 数据范围为零 | 设置 `range = 1` 防止除零 |
| NaN/undefined 值 | 使用 `0` 替代 |
| Canvas 尺寸为零 | 跳过绘制，等待 resize |
| WebSocket 断线 | 自动重连，数据缓存继续累积 |
| 设备离线 | 保留最后状态，标记为 stale |

---

## 8. 总结

### 核心技术要点

1. **固定长度循环缓冲**：600 点（约 3 秒 @ 200Hz），自动丢弃旧数据
2. **Canvas 全量重绘**：每帧清空画布 → 绘制网格 → 绘制波形
3. **动态范围自适应**：根据当前显示窗口数据自动调整幅度
4. **从右向左绘制**：新数据在右侧，符合心电图观看习惯
5. **性能优化**：`requestAnimationFrame` + 防抖 + `shallowRef`

### 架构优势

- **低延迟**：数据到达后立即更新，无中间缓冲延迟
- **高可靠**：断线自动重连，数据不丢失
- **可扩展**：支持多导联、多床位同时显示
- **易维护**：组件化设计，职责清晰