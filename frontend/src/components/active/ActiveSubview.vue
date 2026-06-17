<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoomStore } from '@/stores/room'
import TurnButton from './TurnButton.vue'
import TurnEmblem from './TurnEmblem.vue'
import NudgeButton from './NudgeButton.vue'
import AppButton from '@/components/ui/AppButton.vue'
import Avatar from '@/components/ui/Avatar.vue'

const { t } = useI18n()
const store = useRoomStore()
const currentName = computed(() => store.currentPlayer?.name ?? '')

// "Wait my turn" declines the claim prompt: a purely local dismissal that drops
// the next-up player to the passive watch view. Each new turn re-offers it.
const claimDismissed = ref(false)
watch(() => store.room?.current_player_id, () => { claimDismissed.value = false })
</script>

<template>
  <section class="active" v-if="store.room">
    <!-- 05 YOUR TURN -->
    <div v-if="store.isMyTurn" class="hero">
      <div class="eyebrow">{{ t('active.yourTurnEyebrow') }}</div>
      <TurnEmblem />
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
      <div class="eyebrow">{{ t('active.currentTurn') }}</div>
      <Avatar :name="currentName" :size="96" />
      <h1>{{ t('active.turnOf', { name: currentName }) }}</h1>
      <div class="away">{{ t('active.away', { n: store.playersAway }) }}</div>
      <div class="spacer" />
      <NudgeButton :name="currentName" />
      <p class="nudge-hint">{{ t('active.nudgeHint') }}</p>
    </div>
  </section>
</template>

<style scoped>
.active { min-height: 100%; }
.hero { min-height: 100%; display: flex; flex-direction: column; align-items: center; text-align: center; padding: 30px 24px; background: var(--tt-accent); color: var(--tt-on-accent); }
.eyebrow { font-family: var(--tt-font-mono); font-size: 11px; letter-spacing: .18em; }
.hero-title { font-size: 46px; font-weight: 800; letter-spacing: -.035em; margin: 38px 0 0; }
.hero-hint { font-weight: 600; opacity: .72; }
.spacer { flex: 1; }
.claim, .watch { min-height: 100%; display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--tt-3); padding: 30px 24px; }
.pill { display: inline-flex; padding: 8px 15px; border-radius: var(--tt-r-full); background: rgba(52,211,153,.12); border: 1px solid rgba(52,211,153,.3); color: var(--tt-accent); font-family: var(--tt-font-mono); font-size: 12px; font-weight: 700; }
.away { display: inline-flex; gap: 9px; padding: 9px 16px; border-radius: var(--tt-r-full); background: var(--tt-surface-1); border: 1px solid var(--tt-surface-2); color: var(--tt-accent); font-family: var(--tt-font-mono); font-weight: 700; }
.nudge-hint { font-size: 12px; color: var(--tt-text-faint); }
.claim :deep(.btn) { margin-top: var(--tt-2); }
</style>
