<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import ShareCodeBlock from './ShareCodeBlock.vue'
import PlayerRow from '@/components/player/PlayerRow.vue'
import AppButton from '@/components/ui/AppButton.vue'

const { t } = useI18n()
const store = useRoomStore()
const hostName = computed(() => store.room?.players.find((p) => p.is_host)?.name ?? '')
</script>

<template>
  <section class="lobby" v-if="store.room">
    <template v-if="store.isHost">
      <ShareCodeBlock :code="store.room.code" />
      <div class="hdr">
        <span>{{ t('lobby.players') }} · {{ store.room.players.length }}</span>
        <span class="muted">{{ t('lobby.dragToReorder') }}</span>
      </div>
      <div class="list">
        <PlayerRow
          v-for="p in store.room.players"
          :key="p.id"
          :player="p"
          :is-you="p.id === store.me?.playerId"
          draggable
        />
      </div>
      <AppButton class="start" @click="store.startGame()">{{ t('lobby.startGame') }}</AppButton>
    </template>

    <template v-else>
      <div class="waiting">
        <span class="spinner" aria-hidden="true" />
        <h2>{{ t('lobby.waitingForHost', { host: hostName }) }}</h2>
        <p>{{ t('lobby.waitingHint') }}</p>
      </div>
      <div class="list">
        <PlayerRow
          v-for="p in store.room.players"
          :key="p.id"
          :player="p"
          :is-you="p.id === store.me?.playerId"
        />
      </div>
    </template>
  </section>
</template>

<style scoped>
.lobby { display: flex; flex-direction: column; gap: var(--tt-4); padding: 18px 22px; }
.hdr { display: flex; justify-content: space-between; font-size: 13px; font-weight: 700; color: var(--tt-text); }
.muted { font-family: var(--tt-font-mono); font-size: 11px; color: var(--tt-text-faint); }
.list { display: flex; flex-direction: column; gap: 8px; }
.start { margin-top: var(--tt-4); }
.waiting { display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--tt-3); padding: var(--tt-7) 0; }
.waiting h2 { font-size: 23px; font-weight: 700; margin: 0; }
.waiting p { color: var(--tt-text-muted); max-width: 240px; margin: 0; }
.spinner { width: 40px; height: 40px; border-radius: 50%; border: 2px solid var(--tt-surface-2); border-top-color: var(--tt-accent); animation: ttSpin 1s linear infinite; }
@media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
</style>
