<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import ShareCodeBlock from './ShareCodeBlock.vue'
import PlayerRow from '@/components/player/PlayerRow.vue'
import SupportLink from '@/components/ui/SupportLink.vue'
import DraggablePlayerList from '@/components/player/DraggablePlayerList.vue'
import AppButton from '@/components/ui/AppButton.vue'

const { t } = useI18n()
const store = useRoomStore()
const hostName = computed(() => store.room?.players.find((p) => p.is_host)?.name ?? '')
</script>

<template>
  <section class="lobby" v-if="store.room">
    <template v-if="store.isHost">
      <ShareCodeBlock :code="store.room.code" />
      <button
        class="lock"
        :class="{ on: store.locked }"
        :aria-pressed="store.locked"
        :aria-label="t('lobby.lockRoomA11y')"
        data-test="lock-toggle"
        @click="store.setLocked(!store.locked)"
      >
        <span class="lock-ico" aria-hidden="true">{{ store.locked ? '🔒' : '🔓' }}</span>
        <span class="lock-text">
          <span class="lock-title">{{ store.locked ? t('lobby.lockedTitle') : t('lobby.openTitle') }}</span>
          <span class="lock-hint">{{ store.locked ? t('lobby.lockedHint') : t('lobby.openHint') }}</span>
        </span>
        <span class="lock-switch" aria-hidden="true"><span class="knob" /></span>
      </button>
      <div class="hdr">
        <span>{{ t('lobby.players') }} · {{ store.room.players.length }}</span>
        <button
          class="shuffle"
          data-test="shuffle"
          :aria-label="t('lobby.shuffleA11y')"
          @click="store.shufflePlayers()"
        >
          <span class="sh-ico" aria-hidden="true">⇄</span>{{ t('lobby.shuffle') }}
        </button>
      </div>
      <DraggablePlayerList
        :players="store.room.players"
        :me-id="store.me?.playerId"
        @reorder="store.setOrder($event)"
      />
      <!-- The drag affordance is easiest to read directly beneath the list it describes. -->
      <p class="muted drag-hint">{{ t('lobby.dragToReorder') }}</p>
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

    <SupportLink />
  </section>
</template>

<style scoped>
.lobby { display: flex; flex-direction: column; gap: var(--tt-4); padding: 18px 22px; }
.lock { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 56px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: 1px solid var(--tt-border); padding: 0 16px; cursor: pointer; text-align: left; }
.lock.on { background: rgba(251,191,36,.08); border-color: rgba(251,191,36,.28); }
.lock-ico { font-size: 18px; flex-shrink: 0; }
.lock-text { flex: 1; display: flex; flex-direction: column; }
.lock-title { font-size: 15px; font-weight: 600; color: var(--tt-text); }
.lock-hint { font-size: 12px; color: var(--tt-text-faint); }
.lock-switch { width: 42px; height: 24px; border-radius: var(--tt-r-full); background: var(--tt-surface-0); border: 1px solid var(--tt-border); position: relative; flex-shrink: 0; transition: background .15s ease; }
.lock.on .lock-switch { background: var(--tt-warning); border-color: var(--tt-warning); }
.lock-switch .knob { position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: var(--tt-text); transition: transform .15s ease; }
.lock.on .lock-switch .knob { transform: translateX(18px); background: #1a1a1a; }
@media (prefers-reduced-motion: reduce) { .lock-switch, .lock-switch .knob { transition: none; } }
.hdr { display: flex; justify-content: space-between; align-items: center; font-size: 13px; font-weight: 700; color: var(--tt-text); }
/* 44px like the other tappable chrome — every control here is a thumb target. */
.shuffle { display: inline-flex; align-items: center; gap: 7px; min-height: 44px; padding: 0 14px; border-radius: var(--tt-r-full); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); color: var(--tt-text-muted); font-family: var(--tt-font-mono); font-size: 11px; font-weight: 700; cursor: pointer; }
.shuffle:hover { color: var(--tt-text); }
.sh-ico { font-size: 13px; }
.drag-hint { margin: calc(var(--tt-1) * -1) 0 0; text-align: center; }
.muted { font-family: var(--tt-font-mono); font-size: 11px; color: var(--tt-text-faint); }
.list { display: flex; flex-direction: column; gap: 8px; }
.start { margin-top: var(--tt-4); }
.waiting { display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--tt-3); padding: var(--tt-7) 0; }
.waiting h2 { font-size: 23px; font-weight: 700; margin: 0; }
.waiting p { color: var(--tt-text-muted); max-width: 240px; margin: 0; }
.spinner { width: 40px; height: 40px; border-radius: 50%; border: 2px solid var(--tt-surface-2); border-top-color: var(--tt-accent); animation: ttSpin 1s linear infinite; }
/* No motion: show a full faint accent ring rather than a frozen partial spinner. */
@media (prefers-reduced-motion: reduce) { .spinner { animation: none; border-color: var(--tt-accent); opacity: .4; } }
</style>
