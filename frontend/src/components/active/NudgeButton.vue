<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'

const props = defineProps<{ name: string }>()
const { t } = useI18n()
const store = useRoomStore()
const cooling = computed(() => store.nudgeCooldownRemaining > 0)
const pct = computed(() => (store.nudgeCooldownRemaining / 10) * 100)
const label = computed(() =>
  cooling.value
    ? t('active.nudgeWait', { n: Math.ceil(store.nudgeCooldownRemaining) })
    : t('active.nudge', { name: props.name }),
)
</script>

<template>
  <button class="nudge" :class="{ cooling }" @click="store.nudge()">
    <span v-if="cooling" class="bar" :style="{ width: pct + '%' }" aria-hidden="true" />
    <span class="emoji" aria-hidden="true">👋</span>
    <span class="label">{{ label }}</span>
  </button>
</template>

<style scoped>
.nudge {
  position: relative; width: 100%; min-height: 62px; cursor: pointer;
  border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: 1.5px solid var(--tt-border);
  display: flex; align-items: center; justify-content: center; gap: 11px; overflow: hidden;
  color: var(--tt-text); font-size: 17px; font-weight: 700;
}
.nudge.cooling .label { color: var(--tt-warning); }
.bar { position: absolute; left: 0; top: 0; bottom: 0; background: rgba(251, 191, 36, .16); transition: width .1s linear; }
.emoji, .label { position: relative; }
</style>
