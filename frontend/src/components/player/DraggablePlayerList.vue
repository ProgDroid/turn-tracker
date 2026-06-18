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
// Live insertion target while dragging; drives the gap the other rows open up.
const overIndex = ref(-1)
let fromIndex = -1
let pointerStartY = 0
// Row layout captured at drag start, so the live preview and the drop both
// resolve against the original (un-transformed) positions — stable even as the
// other rows animate apart.
let startRects: DOMRect[] = []
let slotStride = 0

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
  startRects = rowRects()
  slotStride = startRects.length > 1 ? startRects[1].top - startRects[0].top : 0
  overIndex.value = index
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', onUp)
}

function onMove(e: PointerEvent) {
  if (!dragId.value) return
  dragOffset.value = e.clientY - pointerStartY
  overIndex.value = targetIndex(e.clientY, startRects)
}

// How far a non-dragged row at index `i` must slide to open a gap at the
// current drop target: rows the dragged item has passed shift one slot toward
// where it started, revealing where it will land.
function shiftFor(i: number): number {
  const to = overIndex.value
  if (dragId.value === null || to < 0 || to === fromIndex) return 0
  if (to > fromIndex) return i > fromIndex && i <= to ? -slotStride : 0
  return i >= to && i < fromIndex ? slotStride : 0
}

function rowStyle(i: number): Record<string, string> | undefined {
  if (items.value[i].id === dragId.value) {
    return { transform: `translateY(${dragOffset.value}px) scale(1.03)` }
  }
  const dy = shiftFor(i)
  return dy ? { transform: `translateY(${dy}px)` } : undefined
}

function onUp(e: PointerEvent) {
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('pointercancel', onUp)
  const to = targetIndex(e.clientY, startRects)
  const from = fromIndex
  dragId.value = null
  dragOffset.value = 0
  overIndex.value = -1
  fromIndex = -1
  startRects = []
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
      :style="rowStyle(i)"
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
/* Non-dragged rows glide to open a gap at the drop target. */
.drag-item { transition: transform 180ms cubic-bezier(.2, .8, .2, 1); will-change: transform; }
/* The lifted row tracks the finger directly — no transition lag. */
.drag-item.dragging { position: relative; z-index: 5; opacity: .97; box-shadow: var(--tt-shadow-lg); transition: none; }
@media (prefers-reduced-motion: reduce) { .drag-item { transition: none; } }
</style>
