<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'

const { t } = useI18n()
const store = useRoomStore()
const visible = ref(false)
const fromName = ref('')

watch(() => store.nudgeReceivedAt, (at) => {
  if (!at) return
  fromName.value = store.currentPlayer?.name ?? ''
  visible.value = true
  setTimeout(() => (visible.value = false), 3000)
})
</script>

<template>
  <div v-if="visible" class="nudge-toast" role="alert" aria-live="assertive">
    <span class="emoji">👋</span>
    <div>
      <div class="title">{{ t('nudge.nudgedYou', { name: fromName }) }}</div>
      <div class="sub">{{ t('nudge.waiting') }}</div>
    </div>
  </div>
</template>

<style scoped>
.nudge-toast { position: fixed; top: 58px; left: 18px; right: 18px; z-index: 60; display: flex; align-items: center; gap: 14px; border-radius: var(--tt-r-lg); background: var(--tt-on-accent); padding: 16px 18px; box-shadow: var(--tt-shadow-lg); animation: ttShake .6s ease-in-out; }
.title { font-size: 16px; font-weight: 800; color: var(--tt-text); }
.sub { font-size: 13px; color: var(--tt-accent); }
@media (prefers-reduced-motion: reduce) { .nudge-toast { animation: none; } }
</style>
