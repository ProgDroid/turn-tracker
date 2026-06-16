<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'

defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()
const { t } = useI18n()
const store = useRoomStore()

function skip() { const id = store.currentPlayer?.id; if (id) store.skipPlayer(id); emit('close') }
function undo() { store.undoTurn(); emit('close') }
function remove() { const id = store.currentPlayer?.id; if (id) store.removePlayer(id); emit('close') }
</script>

<template>
  <div v-if="open" class="backdrop" @click.self="emit('close')">
    <div class="sheet" role="dialog" aria-modal="true">
      <div class="grab" />
      <button class="item" @click="skip">{{ t('host.skipTurn') }}</button>
      <button class="item" @click="undo">{{ t('host.undoTurn') }}</button>
      <button class="item danger" @click="remove">{{ t('host.remove') }}</button>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.55); z-index: 70; display: flex; align-items: flex-end; }
.sheet { width: 100%; border-radius: 26px 26px 0 0; background: var(--tt-surface-1); border-top: 1px solid var(--tt-border); padding: 14px 18px 30px; box-shadow: 0 -20px 50px -10px rgba(0,0,0,.7); }
.grab { width: 40px; height: 5px; border-radius: 3px; background: var(--tt-border); margin: 0 auto 18px; }
.item { width: 100%; min-height: 54px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: none; color: var(--tt-text); font-size: 15px; font-weight: 600; margin-bottom: 8px; cursor: pointer; }
.item.danger { background: rgba(251,113,133,.08); color: var(--tt-danger); border: 1px solid rgba(251,113,133,.2); }
</style>
