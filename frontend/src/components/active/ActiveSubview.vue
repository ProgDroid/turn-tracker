<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import TurnButton from './TurnButton.vue'
import TurnEmblem from './TurnEmblem.vue'
import NudgeButton from './NudgeButton.vue'
import AppButton from '@/components/ui/AppButton.vue'
import Avatar from '@/components/ui/Avatar.vue'
import HostSheet from '@/components/overlays/HostSheet.vue'
import ReorderSheet from '@/components/overlays/ReorderSheet.vue'

const { t } = useI18n()
const store = useRoomStore()
const currentName = computed(() => store.currentPlayer?.name ?? '')

// "Wait my turn" declines the claim prompt: a purely local dismissal that drops
// the next-up player to the passive watch view. Each new turn re-offers it.
const claimDismissed = ref(false)
watch(() => store.room?.current_player_id, () => { claimDismissed.value = false })

// Host override sheets (screen 09). The corner button opens the action sheet;
// the "Reorder" action swaps to the drag sheet.
const hostSheetOpen = ref(false)
const reorderOpen = ref(false)
function openReorder() { hostSheetOpen.value = false; reorderOpen.value = true }

// Nudge received → emblem pulse speeds to 1.6s for 3s (design Motion notes).
// The nudge lands on the current player, who is on the hero (your-turn) screen.
const emblemFast = ref(false)
let fastTimer: ReturnType<typeof setTimeout> | null = null
watch(() => store.nudgeReceivedAt, (at) => {
  if (!at) return
  emblemFast.value = true
  if (fastTimer) clearTimeout(fastTimer)
  fastTimer = setTimeout(() => { emblemFast.value = false; fastTimer = null }, 3000)
})
onUnmounted(() => { if (fastTimer) clearTimeout(fastTimer) })

// Up-next chain for the watching screen (everyone after the current player).
const upNext = computed(() => store.upNext.slice(0, 4))
const playerCount = computed(() => store.room?.players.length ?? 0)
</script>

<template>
  <section class="active" v-if="store.room">
    <button
      v-if="store.isHost"
      class="host-btn"
      :aria-label="t('host.openA11y')"
      @click="hostSheetOpen = true"
    >
      <span class="gear" aria-hidden="true">⚙</span>
      {{ t('host.open') }}
    </button>

    <!-- 05 YOUR TURN -->
    <div v-if="store.isMyTurn" class="hero">
      <div class="eyebrow">{{ t('active.yourTurnEyebrow') }}</div>
      <TurnEmblem :fast="emblemFast" />
      <h1 class="hero-title" aria-live="assertive">{{ t('active.yourTurnTitle') }}</h1>
      <p class="hero-hint">{{ t('active.yourTurnHint') }}</p>
      <div class="spacer" />
      <TurnButton @done="store.endTurn()" />
    </div>

    <!-- 07 CLAIM -->
    <div v-else-if="store.amINext && !claimDismissed" class="claim">
      <div class="pill">{{ t('active.youreUpNext') }}</div>
      <h1>{{ t('active.finishingTurn', { name: currentName }) }}</h1>
      <p>{{ t('active.claimHint') }}</p>
      <div class="spacer" />
      <AppButton @click="store.claimTurn()">{{ t('active.claim') }}</AppButton>
      <AppButton variant="ghost" @click="claimDismissed = true">{{ t('active.wait') }}</AppButton>
    </div>

    <!-- 06 NOT YOUR TURN -->
    <div v-else class="watch">
      <div class="roomline">
        <span class="rtag">{{ t('active.roomTag', { code: store.room.code }) }}</span>
        <span class="rcount">{{ t('active.playerCount', { n: playerCount }) }}</span>
      </div>
      <div class="watch-body">
        <div class="eyebrow dark">{{ t('active.currentTurn') }}</div>
        <Avatar :name="currentName" :size="96" />
        <h1>{{ t('active.turnOf', { name: currentName }) }}</h1>
        <div class="away">{{ t('active.away', { n: store.playersAway }) }}</div>
      </div>

      <div v-if="upNext.length" class="upnext">
        <div class="upnext-label">{{ t('active.upNext') }}</div>
        <div class="upnext-chain">
          <template v-for="(p, i) in upNext" :key="p.id">
            <span v-if="i > 0" class="arrow" aria-hidden="true">→</span>
            <span class="chip" :class="{ me: p.id === store.me?.playerId }">
              <Avatar :name="p.id === store.me?.playerId ? t('active.youLabel') : p.name" :size="26" :accent="i === 0" />
              <span class="cname">{{ p.id === store.me?.playerId ? t('active.youLabel') : p.name }}</span>
            </span>
          </template>
        </div>
      </div>

      <NudgeButton :name="currentName" />
      <p class="nudge-hint">{{ t('active.nudgeHint') }}</p>
    </div>

    <HostSheet :open="hostSheetOpen" @close="hostSheetOpen = false" @reorder="openReorder" />
    <ReorderSheet :open="reorderOpen" @close="reorderOpen = false" />
  </section>
</template>

<style scoped>
.active { position: relative; min-height: 100%; }
.host-btn {
  position: absolute; top: 14px; right: 14px; z-index: 30;
  display: inline-flex; align-items: center; gap: 7px; min-height: 44px; padding: 0 14px;
  border-radius: var(--tt-r-full); border: 1px solid rgba(255,255,255,.16);
  background: rgba(20,20,24,.55); color: #f4f4f5;
  font-family: var(--tt-font-mono); font-size: 12px; font-weight: 700; letter-spacing: .04em; cursor: pointer;
}
.gear { font-size: 14px; }

.hero { min-height: 100%; display: flex; flex-direction: column; align-items: center; text-align: center; padding: 30px 24px; background: var(--tt-accent); color: var(--tt-on-accent); animation: ttHeroWipe var(--tt-dur-hero) var(--tt-ease) both; }
.eyebrow { font-family: var(--tt-font-mono); font-size: 11px; letter-spacing: .18em; text-transform: uppercase; }
.eyebrow.dark { color: var(--tt-text-muted); }
.hero-title { font-size: 46px; font-weight: 800; letter-spacing: -.035em; margin: 38px 0 0; animation: ttRise var(--tt-dur-hero) var(--tt-ease) both; }
.hero-hint { font-weight: 600; opacity: .72; }
.spacer { flex: 1; }

.claim, .watch { min-height: 100%; display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--tt-3); padding: 30px 24px; }
.pill { display: inline-flex; padding: 8px 15px; border-radius: var(--tt-r-full); background: rgba(52,211,153,.12); border: 1px solid rgba(52,211,153,.3); color: var(--tt-accent); font-family: var(--tt-font-mono); font-size: 12px; font-weight: 700; }

.watch { gap: var(--tt-2); }
.roomline { width: 100%; display: flex; justify-content: space-between; align-items: center; font-family: var(--tt-font-mono); font-size: 12px; letter-spacing: .1em; color: var(--tt-text-faint); }
.watch-body { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--tt-3); }
.away { display: inline-flex; gap: 9px; padding: 9px 16px; border-radius: var(--tt-r-full); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); color: var(--tt-accent); font-family: var(--tt-font-mono); font-weight: 700; }

.upnext { width: 100%; border-radius: var(--tt-r-lg); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); padding: 14px 16px; }
.upnext-label { font-family: var(--tt-font-mono); font-size: 10px; letter-spacing: .12em; text-transform: uppercase; color: var(--tt-text-faint); margin-bottom: 12px; }
.upnext-chain { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.chip { display: inline-flex; align-items: center; gap: 6px; }
.chip.me { padding: 3px 8px 3px 3px; border-radius: var(--tt-r-full); background: rgba(52,211,153,.12); }
.cname { font-size: 13px; color: var(--tt-text-muted); }
.chip.me .cname { font-weight: 700; color: var(--tt-accent); }
.arrow { color: var(--tt-border); }

.nudge-hint { font-size: 12px; color: var(--tt-text-faint); }
.claim :deep(.btn) { margin-top: var(--tt-2); }

@media (prefers-reduced-motion: reduce) {
  .hero { animation: ttFade var(--tt-dur-fast) both; }
  .hero-title { animation: none; }
}
</style>
