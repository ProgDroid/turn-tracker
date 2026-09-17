<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import Avatar from '@/components/ui/Avatar.vue'

defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'reorder'): void }>()
const { t } = useI18n()
const store = useRoomStore()

const currentName = computed(() => store.currentPlayer?.name ?? '')
// Removing the only player disbands the room, so name the action after what it
// actually does. "Remove from room" implies the room carries on without you.
const isLastPlayer = computed(() => (store.room?.players.length ?? 0) <= 1)

function skip() { const id = store.currentPlayer?.id; if (id) store.skipPlayer(id); emit('close') }
function undo() { store.undoTurn(); emit('close') }
function reorder() { emit('reorder') }
function shuffleOrder() { store.shufflePlayers(); emit('close') }
function remove() { const id = store.currentPlayer?.id; if (id) store.removePlayer(id); emit('close') }
function toggleLock() { store.setLocked(!store.locked); emit('close') }
</script>

<template>
  <div v-if="open" class="backdrop" @click.self="emit('close')">
    <div class="sheet" role="dialog" aria-modal="true" :aria-label="t('host.title')">
      <div class="grab" />

      <header class="current">
        <Avatar :name="currentName" :size="38" />
        <div class="who">
          <div class="cname">{{ currentName }}</div>
          <div class="ctaking">{{ t('host.taking') }}</div>
        </div>
        <span class="badge">{{ t('host.current') }}</span>
      </header>

      <div class="items">
        <button class="item" @click="skip">
          <span class="ico skip" aria-hidden="true">»</span>
          <span class="body"><span class="label">{{ t('host.skipTurn') }}</span></span>
          <span class="hint">{{ t('host.skipHint', { name: currentName }) }}</span>
        </button>
        <button class="item" @click="undo">
          <span class="ico undo" aria-hidden="true">↻</span>
          <span class="body"><span class="label">{{ t('host.undoTurn') }}</span></span>
          <span class="hint">{{ t('host.undoHint') }}</span>
        </button>
        <button class="item" @click="reorder">
          <span class="ico reorder" aria-hidden="true"><i /><i /><i /></span>
          <span class="body"><span class="label">{{ t('host.reorder') }}</span></span>
          <span class="hint">{{ t('host.reorderHint') }}</span>
        </button>
        <button class="item" data-test="shuffle" @click="shuffleOrder">
          <span class="ico shuffle" aria-hidden="true">⇄</span>
          <span class="body"><span class="label">{{ t('host.shuffle') }}</span></span>
          <span class="hint">{{ t('host.shuffleHint') }}</span>
        </button>
        <button class="item" data-test="lock-toggle" @click="toggleLock">
          <span class="ico lock" aria-hidden="true">{{ store.locked ? '🔒' : '🔓' }}</span>
          <span class="body">
            <span class="label">{{ store.locked ? t('host.unlockRoom') : t('host.lockRoom') }}</span>
          </span>
          <span class="hint">{{ store.locked ? t('host.unlockRoomHint') : t('host.lockRoomHint') }}</span>
        </button>
        <button class="item danger" data-test="remove" @click="remove">
          <span class="ico remove" aria-hidden="true">×</span>
          <span class="body">
            <span class="label">{{ isLastPlayer ? t('host.closeRoom') : t('host.remove') }}</span>
          </span>
          <span v-if="isLastPlayer" class="hint">{{ t('host.closeRoomHint') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.55); z-index: 70; display: flex; align-items: flex-end; }
.sheet { width: 100%; border-radius: 26px 26px 0 0; background: var(--tt-surface-1); border-top: 1px solid var(--tt-border); padding: 14px 18px 30px; box-shadow: 0 -20px 50px -10px rgba(0,0,0,.7); }
.grab { width: 40px; height: 5px; border-radius: 3px; background: var(--tt-border); margin: 0 auto 18px; }

.current { display: flex; align-items: center; gap: 12px; margin-bottom: 18px; padding: 0 4px; }
.who { flex: 1; }
.cname { font-size: 17px; font-weight: 700; color: var(--tt-text); }
.ctaking { font-size: 13px; color: var(--tt-accent); }
.badge { padding: 4px 10px; border-radius: var(--tt-r-full); background: rgba(52,211,153,.12); font-family: var(--tt-font-mono); font-size: 10px; font-weight: 700; color: var(--tt-accent); }

.items { display: flex; flex-direction: column; gap: 8px; }
.item { display: flex; align-items: center; gap: 13px; width: 100%; min-height: 56px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: none; padding: 0 16px; cursor: pointer; text-align: left; }
.item.danger { background: rgba(251,113,133,.08); border: 1px solid rgba(251,113,133,.2); }
.body { flex: 1; }
.label { font-size: 15px; font-weight: 600; color: var(--tt-text); }
.item.danger .label { color: var(--tt-danger); }
.hint { font-size: 12px; color: var(--tt-text-faint); }

.ico { width: 24px; height: 24px; border-radius: 6px; display: flex; align-items: center; justify-content: center; font-weight: 800; flex-shrink: 0; }
.ico.skip { background: rgba(251,191,36,.16); color: var(--tt-warning); font-size: 14px; }
.ico.lock { background: rgba(251,191,36,.12); font-size: 14px; }
.ico.undo { background: rgba(56,189,248,.16); color: #38bdf8; font-size: 15px; }
.ico.remove { background: rgba(251,113,133,.16); color: var(--tt-danger); font-size: 16px; }
.ico.shuffle { background: rgba(167,139,250,.16); color: #a78bfa; font-size: 15px; }
.ico.reorder { flex-direction: column; gap: 3px; background: none; }
.ico.reorder i { width: 15px; height: 2px; background: var(--tt-text-muted); border-radius: 2px; }
</style>
