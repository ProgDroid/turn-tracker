<script setup lang="ts">
import type { PublicPlayer } from '@/types/wire'
import Avatar from '@/components/ui/Avatar.vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  player: PublicPlayer
  skipped?: boolean
  isYou?: boolean
  draggable?: boolean
}>()
const emit = defineEmits<{ (e: 'handleDown', ev: PointerEvent): void }>()
const { t } = useI18n()
</script>

<template>
  <div class="row" :class="{ dim: !player.connected, skipped: skipped && !player.is_host }">
    <span
      v-if="draggable"
      class="handle"
      aria-hidden="true"
      @pointerdown="emit('handleDown', $event)"
    ><i /><i /><i /></span>
    <Avatar :name="player.name" :accent="player.is_host" />
    <span class="name">
      {{ player.name }}
      <span v-if="isYou" class="you">{{ t('lobby.you') }}</span>
    </span>

    <span v-if="player.is_host" class="badge host">{{ t('lobby.host') }}</span>
    <span v-else-if="skipped" class="badge skip">» {{ t('status.skipped') }}</span>
    <span v-else class="status-label">
      {{ player.connected ? t('status.connected') : t('status.disconnected') }}
    </span>

    <span class="dot" :class="player.connected ? 'on' : 'off'" aria-hidden="true" />
  </div>
</template>

<style scoped>
.row {
  display: flex; align-items: center; gap: var(--tt-3);
  min-height: 56px; padding: 0 14px;
  border-radius: var(--tt-r-md); background: var(--tt-surface-2);
  border: 1px solid var(--tt-border);
}
.row.dim { opacity: 0.65; background: var(--tt-surface-1); }
.row.skipped { border-color: rgba(251, 191, 36, 0.25); }
.handle { display: flex; flex-direction: column; gap: 3px; padding: 8px 4px; margin: -8px -4px; touch-action: none; cursor: grab; }
.handle:active { cursor: grabbing; }
.handle i { width: 16px; height: 2px; background: var(--tt-text-faint); border-radius: 2px; }
.name { flex: 1; font-size: 15px; font-weight: 600; color: var(--tt-text); }
.you { color: var(--tt-accent); font-weight: 700; font-size: 13px; }
.badge { padding: 3px 9px; border-radius: var(--tt-r-full); font-family: var(--tt-font-mono); font-size: 10px; font-weight: 700; }
.badge.host { background: rgba(52, 211, 153, 0.14); color: var(--tt-accent); }
.badge.skip { background: rgba(251, 191, 36, 0.14); color: var(--tt-warning); }
.status-label { font-size: 11px; color: var(--tt-text-faint); }
.dot { width: 8px; height: 8px; border-radius: 50%; }
.dot.on { background: var(--tt-accent); }
.dot.off { background: var(--tt-text-faint); }
</style>
