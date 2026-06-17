<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import PlayerRow from '@/components/player/PlayerRow.vue'

defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()
const { t } = useI18n()
const store = useRoomStore()

// Mirror LobbySubview's drag-to-reorder: track the dragged index locally and
// commit the new order via set_order (allowed in active state server-side).
const dragIndex = ref<number | null>(null)
function onDragStart(i: number) { dragIndex.value = i }
function onDrop(to: number) {
  const from = dragIndex.value
  dragIndex.value = null
  if (from === null || from === to || !store.room) return
  const ids = store.room.players.map((p) => p.id)
  const [moved] = ids.splice(from, 1)
  ids.splice(to, 0, moved)
  store.setOrder(ids)
}
</script>

<template>
  <div v-if="open" class="backdrop" @click.self="emit('close')">
    <div class="sheet" role="dialog" aria-modal="true" :aria-label="t('host.reorder')">
      <div class="grab" />
      <header class="hdr">
        <span class="title">{{ t('host.reorder') }}</span>
        <span class="muted">{{ t('lobby.dragToReorder') }}</span>
      </header>
      <div v-if="store.room" class="list">
        <PlayerRow
          v-for="(p, i) in store.room.players"
          :key="p.id"
          :player="p"
          :is-you="p.id === store.me?.playerId"
          draggable
          @dragstart="onDragStart(i)"
          @dragover.prevent
          @drop="onDrop(i)"
        />
      </div>
      <button class="done" @click="emit('close')">{{ t('host.done') }}</button>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.55); z-index: 70; display: flex; align-items: flex-end; }
.sheet { width: 100%; border-radius: 26px 26px 0 0; background: var(--tt-surface-1); border-top: 1px solid var(--tt-border); padding: 14px 18px 30px; box-shadow: 0 -20px 50px -10px rgba(0,0,0,.7); }
.grab { width: 40px; height: 5px; border-radius: 3px; background: var(--tt-border); margin: 0 auto 18px; }
.hdr { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 14px; padding: 0 2px; }
.title { font-size: 15px; font-weight: 700; color: var(--tt-text); }
.muted { font-family: var(--tt-font-mono); font-size: 11px; color: var(--tt-text-faint); }
.list { display: flex; flex-direction: column; gap: 8px; }
.done { width: 100%; min-height: 56px; margin-top: 16px; border: none; border-radius: var(--tt-r-md); background: var(--tt-accent); color: var(--tt-on-accent); font-size: 16px; font-weight: 700; cursor: pointer; }
</style>
