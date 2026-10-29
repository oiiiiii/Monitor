<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps<{
  visible: boolean
  bedNo: number
  currentName: string
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'apply', name: string): void
  (e: 'clear'): void
}>()

const nameInput = ref('')

watch(() => props.visible, (val) => {
  if (val) {
    nameInput.value = props.currentName
  }
})

function handleApply() {
  emit('apply', nameInput.value.trim())
  emit('close')
}

function handleClear() {
  emit('clear')
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
          <span class="modal-title">{{ bedNo }} 床 - 患者信息</span>
          <button class="close-btn" @click="handleClose">×</button>
        </div>
        <div class="modal-body">
          <div class="form-row">
            <label class="form-label">患者姓名</label>
            <input
              type="text"
              v-model="nameInput"
              class="form-input"
              placeholder="请输入患者姓名"
              @keyup.enter="handleApply"
            />
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-danger" @click="handleClear">解除病人</button>
          <div class="footer-right">
            <button class="btn btn-secondary" @click="handleClose">取消</button>
            <button class="btn btn-primary" @click="handleApply">应用</button>
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
  width: 400px;
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
  padding: 20px;
}

.form-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.form-label {
  font-size: 13px;
  color: #aaa;
}

.form-input {
  padding: 8px 12px;
  background: #111;
  border: 1px solid #444;
  border-radius: 4px;
  color: #fff;
  font-size: 14px;
}

.form-input:focus {
  outline: none;
  border-color: #4a8aca;
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

.btn-danger {
  background: #8a2a2a;
  color: #fff;
  border-color: #ca4a4a;
}

.btn-danger:hover {
  background: #a03a3a;
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
