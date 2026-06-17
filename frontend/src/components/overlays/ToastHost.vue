<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import { errorKey } from '@/i18n'
import Toast from '@/components/ui/Toast.vue'

const { t } = useI18n()
const store = useRoomStore()
const errorText = computed(() => (store.lastError ? t(errorKey(store.lastError.code)) : ''))

watch(() => store.lastError, (e) => {
  if (e) setTimeout(() => store.dismissError(), 4000)
})
</script>

<template>
  <div class="host">
    <Toast v-if="store.connStatus === 'reconnecting'" tone="info">
      {{ t('errors.reconnecting') }}
    </Toast>
    <Toast v-if="store.lastError" :tone="store.lastError.code === 'nudge_cooldown' ? 'warning' : 'danger'">
      {{ errorText }}
    </Toast>
  </div>
</template>

<style scoped>
.host { position: fixed; top: 12px; left: 12px; right: 12px; z-index: 50; display: flex; flex-direction: column; gap: 8px; pointer-events: none; }
.host :deep(.toast) { pointer-events: auto; }
</style>
