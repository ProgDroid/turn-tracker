<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { useRoomStore } from '@/stores/room'
import { errorKey } from '@/i18n'
import { loadToken } from '@/services/tokenStore'
import LobbySubview from '@/components/lobby/LobbySubview.vue'
import ActiveSubview from '@/components/active/ActiveSubview.vue'
import ToastHost from '@/components/overlays/ToastHost.vue'
import NudgeToast from '@/components/overlays/NudgeToast.vue'
import AppButton from '@/components/ui/AppButton.vue'
import { vibrate } from '@/services/haptics'
import { chime } from '@/services/sound'
import { acquire, release } from '@/services/wakeLock'

const props = defineProps<{ code: string }>()
const { t } = useI18n()
const router = useRouter()
const store = useRoomStore()

// Newcomers arriving via a shared link (no saved token, no name handed over from
// the landing page) get a name prompt before we join — keeps room URLs clean.
const needsName = ref(false)
const joinName = ref('')

onMounted(() => {
  store.connect(props.code)
  const stateName =
    typeof history.state?.name === 'string' ? (history.state.name as string) : undefined
  // A saved token means a returning player → rejoin by token (name comes from the
  // server). A handed-over name means an in-app joiner. Either way, join now.
  if (loadToken(props.code) || stateName) {
    // join is sent once the socket is open; queued by the socket until then.
    queueMicrotask(() => store.join(stateName))
  } else {
    needsName.value = true
  }
})

function submitName() {
  const name = joinName.value.trim()
  if (!name) return
  needsName.value = false
  store.join(name)
}

onUnmounted(() => { void release() })

// Device effects on becoming / leaving your turn.
watch(
  () => store.isMyTurn,
  (mine, was) => {
    if (mine && !was) {
      vibrate([0, 80, 40, 80])
      chime()
      void acquire()
    } else if (!mine && was) {
      void release()
    }
  },
)

// Nudge received → double buzz.
watch(() => store.nudgeReceivedAt, (at) => { if (at) vibrate([0, 60, 40, 60]) })

// Room gone (stale token / server restart) → back to landing.
watch(() => store.roomGone, (gone) => { if (gone) router.replace('/') })
</script>

<template>
  <main class="room">
    <ToastHost />
    <NudgeToast />
    <section v-if="store.joinRejectedCode" class="rejected">
      <div class="brand"><span class="dot" />{{ t('appName') }}</div>
      <h1>{{ t('room.cantJoin') }}</h1>
      <p>{{ t(errorKey(store.joinRejectedCode)) }}</p>
      <AppButton data-test="rejected-home" @click="router.replace('/')">
        {{ t('room.backHome') }}
      </AppButton>
    </section>
    <section v-else-if="needsName" class="namegate">
      <div class="brand"><span class="dot" />{{ t('appName') }}</div>
      <h1>{{ t('room.joinHeading') }}</h1>
      <p>{{ t('room.joinSub', { code: code.toUpperCase() }) }}</p>
      <label>{{ t('landing.yourName') }}</label>
      <input
        v-model="joinName"
        class="field"
        data-test="join-name"
        @keyup.enter="submitName"
      />
      <AppButton data-test="join-submit" :disabled="!joinName.trim()" @click="submitName">
        {{ t('room.joinCta') }}
      </AppButton>
    </section>
    <LobbySubview v-else-if="store.phase === 'lobby'" />
    <ActiveSubview v-else-if="store.phase === 'active'" />
    <div v-else class="connecting">{{ t('lobby.connecting') }}</div>
  </main>
</template>

<style scoped>
.room { min-height: 100%; background: var(--tt-surface-0); }
.connecting { display: flex; align-items: center; justify-content: center; min-height: 100vh; color: var(--tt-text-muted); }
.namegate, .rejected { max-width: 420px; margin: 0 auto; padding: 56px 26px; display: flex; flex-direction: column; gap: var(--tt-3); min-height: 100vh; min-height: 100dvh; }
.namegate .brand, .rejected .brand { display: inline-flex; align-items: center; gap: 10px; font-family: var(--tt-font-mono); font-weight: 700; letter-spacing: .12em; color: var(--tt-text); }
.namegate .dot, .rejected .dot { width: 13px; height: 13px; border-radius: 50%; background: var(--tt-accent); box-shadow: 0 0 12px var(--tt-accent); }
.namegate h1, .rejected h1 { font-size: 34px; font-weight: 800; letter-spacing: -.03em; margin: 24px 0 0; }
.namegate p, .rejected p { color: var(--tt-text-muted); margin: 0; line-height: 1.55; }
.namegate label { font-family: var(--tt-font-mono); font-size: 12px; letter-spacing: .08em; color: var(--tt-text-faint); margin-top: var(--tt-4); }
.namegate .field { height: 56px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: 1.5px solid var(--tt-border); color: var(--tt-text); padding: 0 18px; font-size: 18px; font-weight: 600; }
.namegate .field:focus { outline: none; border-color: var(--tt-accent); }
</style>
