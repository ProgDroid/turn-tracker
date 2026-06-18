<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import QRCode from 'qrcode'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ code: string }>()
const { t } = useI18n()
const copied = ref(false)
const showQr = ref(false)
const canvas = ref<HTMLCanvasElement | null>(null)

function joinUrl() {
  return `${window.location.origin}/room/${props.code}`
}

async function copy() {
  try { await navigator.clipboard.writeText(joinUrl()) } catch { /* no-op */ }
  copied.value = true
  setTimeout(() => (copied.value = false), 2000)
}

async function renderQr() {
  if (showQr.value && canvas.value) await QRCode.toCanvas(canvas.value, joinUrl(), { width: 180 })
}
watch(showQr, renderQr)
onMounted(renderQr)
</script>

<template>
  <div class="share">
    <div class="eyebrow">{{ t('lobby.roomCode') }}</div>
    <div class="code">{{ code }}</div>
    <div class="actions">
      <button @click="copy">{{ copied ? t('lobby.copied') : t('lobby.copyLink') }}</button>
      <button @click="showQr = !showQr">{{ showQr ? t('lobby.hideQr') : t('lobby.showQr') }}</button>
    </div>
    <canvas v-show="showQr" ref="canvas" class="qr" />
  </div>
</template>

<style scoped>
.share { border-radius: var(--tt-r-xl); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); padding: 20px; text-align: center; }
.eyebrow { font-family: var(--tt-font-mono); font-size: 11px; letter-spacing: .14em; color: var(--tt-text-faint); }
.code { margin-top: 12px; font-family: var(--tt-font-mono); font-weight: 700; font-size: 42px; letter-spacing: .16em; color: var(--tt-accent); text-shadow: 0 0 18px rgba(52, 211, 153, 0.35); }
.actions { margin-top: 16px; display: flex; gap: 9px; }
.actions button { flex: 1; height: 42px; border-radius: 11px; background: var(--tt-surface-2); color: var(--tt-text); border: none; font-weight: 600; cursor: pointer; }
.qr { margin: 16px auto 0; border-radius: 8px; }
</style>
