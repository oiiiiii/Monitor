// WebSocket 连接管理
import { ref } from 'vue'
import type { PushMessage, PushType } from '@/types'

function getWsUrl(): string {
  if (import.meta.env.VITE_WS_URL) {
    return import.meta.env.VITE_WS_URL
  }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const host = window.location.hostname
  const port = window.location.port
  if (port === '3000' || port === '5174') {
    return `${protocol}//${host}:8080`
  }
  if (port) {
    return `${protocol}//${host}:${port}`
  }
  return `${protocol}//${host}:8080`
}

export const wsConnected = ref(false)
export const wsConnecting = ref(false)
export const wsReconnecting = ref(false)

type MessageHandler = (msg: PushMessage) => void
const handlers: Set<MessageHandler> = new Set()

let ws: WebSocket | null = null
let reconnectTimer: number | null = null
let heartbeatTimer: number | null = null
let heartbeatTimeoutTimer: number | null = null
let reconnectAttempts = 0
const MAX_RECONNECT_DELAY = 30000
const MAX_RECONNECT_ATTEMPTS = 0
const HEARTBEAT_INTERVAL = 25000
const HEARTBEAT_TIMEOUT = 10000

const subscribedBeds = new Set<number>()

export function connectWs() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return
  }

  const wsUrl = getWsUrl()
  wsConnecting.value = true
  console.log('[WS] 正在连接:', wsUrl)

  try {
    ws = new WebSocket(wsUrl)
  } catch (e) {
    console.error('[WS] 创建连接失败:', e)
    scheduleReconnect()
    return
  }

  ws.onopen = () => {
    console.log('[WS] 连接成功')
    wsConnected.value = true
    wsConnecting.value = false
    wsReconnecting.value = false
    reconnectAttempts = 0

    for (const bed of subscribedBeds) {
      sendSubscribe(bed)
    }

    startHeartbeat()
  }

  ws.onmessage = (event) => {
    resetHeartbeatTimeout()
    try {
      const msg: PushMessage = JSON.parse(event.data)
      for (const handler of handlers) {
        try {
          handler(msg)
        } catch (e) {
          console.error('[WS] 消息处理错误:', e)
        }
      }
    } catch (e) {
      console.error('[WS] 消息解析失败:', e)
    }
  }

  ws.onerror = (e) => {
    console.error('[WS] 连接错误:', e)
  }

  ws.onclose = (e) => {
    console.log('[WS] 连接关闭，code:', e.code, 'reason:', e.reason)
    wsConnected.value = false
    wsConnecting.value = false
    stopHeartbeat()
    stopHeartbeatTimeout()
    if (e.code !== 1000) {
      scheduleReconnect()
    }
  }
}

export function disconnectWs() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  stopHeartbeat()
  stopHeartbeatTimeout()

  if (ws) {
    ws.close()
    ws = null
  }

  wsConnected.value = false
  wsConnecting.value = false
  wsReconnecting.value = false
}

function scheduleReconnect() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
  }

  if (MAX_RECONNECT_ATTEMPTS > 0 && reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
    console.error('[WS] 已达到最大重连次数，停止重连')
    wsReconnecting.value = false
    return
  }

  reconnectAttempts++
  wsReconnecting.value = true
  const baseDelay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), MAX_RECONNECT_DELAY)
  const jitter = Math.random() * 1000
  const delay = baseDelay + jitter
  console.log(`[WS] ${(delay / 1000).toFixed(1)}s 后重连 (第 ${reconnectAttempts} 次)`)

  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null
    connectWs()
  }, delay)
}

function startHeartbeat() {
  stopHeartbeat()
  resetHeartbeatTimeout()
  heartbeatTimer = window.setInterval(() => {
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ action: 'ping', bed_no: 0 }))
    }
  }, HEARTBEAT_INTERVAL)
}

function stopHeartbeat() {
  if (heartbeatTimer) {
    clearInterval(heartbeatTimer)
    heartbeatTimer = null
  }
}

function resetHeartbeatTimeout() {
  stopHeartbeatTimeout()
  heartbeatTimeoutTimer = window.setTimeout(() => {
    console.error('[WS] 心跳超时，主动断开重连')
    if (ws) {
      ws.close(1011, 'heartbeat timeout')
      ws = null
    }
    scheduleReconnect()
  }, HEARTBEAT_TIMEOUT)
}

function stopHeartbeatTimeout() {
  if (heartbeatTimeoutTimer) {
    clearTimeout(heartbeatTimeoutTimer)
    heartbeatTimeoutTimer = null
  }
}

// 发送消息
function send(data: any) {
  if (ws?.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(data))
    return true
  }
  return false
}

// 订阅床位
export function subscribeBed(bedNo: number) {
  subscribedBeds.add(bedNo)
  sendSubscribe(bedNo)
}

// 取消订阅
export function unsubscribeBed(bedNo: number) {
  subscribedBeds.delete(bedNo)
  send({ action: 'unsubscribe', bed_no: bedNo })
}

function sendSubscribe(bedNo: number) {
  send({ action: 'subscribe', bed_no: bedNo })
}

// 注册消息处理器
export function onMessage(handler: MessageHandler) {
  handlers.add(handler)
  return () => handlers.delete(handler)
}

// 取消注册
export function offMessage(handler: MessageHandler) {
  handlers.delete(handler)
}
