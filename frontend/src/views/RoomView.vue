<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useRoomStore } from '@/stores/room'
import LobbySubview from '@/components/lobby/LobbySubview.vue'
import ActiveSubview from '@/components/active/ActiveSubview.vue'
import ToastHost from '@/components/overlays/ToastHost.vue'
import NudgeToast from '@/components/overlays/NudgeToast.vue'
import { vibrate } from '@/services/haptics'
import { chime } from '@/services/sound'
import { acquire, release } from '@/services/wakeLock'

const props = defineProps<{ code: string }>()
const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useRoomStore()

onMounted(() => {
  store.connect(props.code)
  const name = typeof route.query.name === 'string' ? route.query.name : undefined
  // join is sent once the socket is open; the store sends on connect-open via its callback.
  // For simplicity the store joins immediately after connect resolves the first frame;
  // here we trigger join on first open by sending after a microtask.
  queueMicrotask(() => store.join(name))
})

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
    <LobbySubview v-if="store.phase === 'lobby'" />
    <ActiveSubview v-else-if="store.phase === 'active'" />
    <div v-else class="connecting">{{ t('lobby.connecting') }}</div>
  </main>
</template>

<style scoped>
.room { min-height: 100%; background: var(--tt-surface-0); }
.connecting { display: flex; align-items: center; justify-content: center; min-height: 100vh; color: var(--tt-text-muted); }
</style>
