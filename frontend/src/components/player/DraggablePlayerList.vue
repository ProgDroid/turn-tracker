<script setup lang="ts">
import { ref, watch } from 'vue'
import type { PublicPlayer } from '@/types/wire'
import PlayerRow from '@/components/player/PlayerRow.vue'
import { reorder, targetIndex } from '@/services/reorder'

const props = defineProps<{ players: PublicPlayer[]; meId?: string }>()
const emit = defineEmits<{ (e: 'reorder', ids: string[]): void }>()

// Local working copy so the dragged row can follow the finger before we commit.
// Resynced from server state except mid-drag (so a broadcast can't yank it).
const items = ref<PublicPlayer[]>([...props.players])
watch(
  () => props.players,
  (next) => { if (!dragId.value) items.value = [...next] },
  { deep: true },
)

const listEl = ref<HTMLElement | null>(null)
const dragId = ref<string | null>(null)
const dragOffset = ref(0)
let fromIndex = -1
let pointerStartY = 0

function rowRects(): DOMRect[] {
  const els = listEl.value?.querySelectorAll<HTMLElement>('.drag-item')
  return els ? Array.from(els, (el) => el.getBoundingClientRect()) : []
}

// Pointer Events cover both touch and mouse with one path. The handle sets
// `touch-action: none` so dragging it never scrolls the page on mobile.
function onHandleDown(index: number, e: PointerEvent) {
  e.preventDefault()
  fromIndex = index
  dragId.value = items.value[index].id
  pointerStartY = e.clientY
  dragOffset.value = 0
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', onUp)
}

function onMove(e: PointerEvent) {
  if (!dragId.value) return
  dragOffset.value = e.clientY - pointerStartY
}

function onUp(e: PointerEvent) {
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('pointercancel', onUp)
  const to = targetIndex(e.clientY, rowRects())
  const from = fromIndex
  dragId.value = null
  dragOffset.value = 0
  fromIndex = -1
  if (from >= 0 && to !== from) {
    const next = reorder(items.value, from, to)
    items.value = next
    emit('reorder', next.map((p) => p.id))
  }
}
</script>

<template>
  <div ref="listEl" class="drag-list">
    <div
      v-for="(p, i) in items"
      :key="p.id"
      class="drag-item"
      :class="{ dragging: dragId === p.id }"
      :style="dragId === p.id ? { transform: `translateY(${dragOffset}px)` } : undefined"
    >
      <PlayerRow
        :player="p"
        :is-you="p.id === meId"
        draggable
        @handle-down="onHandleDown(i, $event)"
      />
    </div>
  </div>
</template>

<style scoped>
.drag-list { display: flex; flex-direction: column; gap: 8px; position: relative; }
.drag-item.dragging { position: relative; z-index: 5; opacity: .95; box-shadow: var(--tt-shadow-lg); }
</style>
