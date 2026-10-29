<script setup lang="ts">
import MonitorView from '@/views/MonitorView.vue'
import { wsConnected, wsConnecting } from '@/services/websocket'
</script>

<template>
  <div class="app-container">
    <MonitorView />
    <div 
      v-if="!wsConnected && wsConnecting" 
      class="connection-status connecting"
    >
      <span class="status-icon">⏳</span>
      <span>正在重连...</span>
    </div>
    <div 
      v-if="!wsConnected && !wsConnecting" 
      class="connection-status disconnected"
    >
      <span class="status-icon">⚠️</span>
      <span>连接已断开</span>
    </div>
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: #000;
}

.app-container {
  width: 100%;
  height: 100%;
  position: relative;
}

.connection-status {
  position: fixed;
  top: 10px;
  right: 10px;
  padding: 8px 16px;
  border-radius: 4px;
  font-size: 14px;
  font-weight: bold;
  z-index: 1000;
  display: flex;
  align-items: center;
  gap: 8px;
}

.connection-status.connecting {
  background: rgba(255, 165, 0, 0.9);
  color: #000;
  animation: pulse 1s ease-in-out infinite;
}

.connection-status.disconnected {
  background: rgba(255, 0, 0, 0.9);
  color: #fff;
}

.status-icon {
  font-size: 18px;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.6; }
}
</style>
