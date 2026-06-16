<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { saveToken } from '@/services/tokenStore'
import AppButton from '@/components/ui/AppButton.vue'
import CodeInput from '@/components/ui/CodeInput.vue'

const { t } = useI18n()
const router = useRouter()
const mode = ref<'create' | 'join'>('create')
const name = ref('')
const code = ref('')
const error = ref('')

async function create() {
  error.value = ''
  const res = await fetch('/api/rooms', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ host_name: name.value.trim() }),
  })
  if (!res.ok) { error.value = t('errors.generic'); return }
  const data = (await res.json()) as { room_code: string; player_id: string; token: string }
  saveToken(data.room_code, data.token)
  router.push({ path: `/room/${data.room_code}` })
}

function join() {
  if (code.value.length !== 6 || !name.value.trim()) return
  router.push({ path: `/room/${code.value}`, query: { name: name.value.trim() } })
}
</script>

<template>
  <main class="wrap">
    <div class="brand"><span class="dot" />{{ t('appName') }}</div>

    <template v-if="mode === 'create'">
      <h1>{{ t('landing.createTitle') }}</h1>
      <p>{{ t('landing.createSubtitle') }}</p>
      <label>{{ t('landing.yourName') }}</label>
      <input data-test="name" v-model="name" class="field" />
      <AppButton data-test="create" :disabled="!name.trim()" @click="create">
        {{ t('landing.createCta') }}
      </AppButton>
      <button class="link" @click="mode = 'join'">{{ t('landing.joinPrompt') }}</button>
    </template>

    <template v-else>
      <h1>{{ t('landing.joinTitle') }}</h1>
      <p>{{ t('landing.joinSubtitle') }}</p>
      <CodeInput v-model="code" />
      <label>{{ t('landing.yourName') }}</label>
      <input data-test="name" v-model="name" class="field" />
      <AppButton :disabled="code.length !== 6 || !name.trim()" @click="join">
        {{ t('landing.joinCta') }}
      </AppButton>
    </template>

    <p v-if="error" class="err">{{ error }}</p>
  </main>
</template>

<style scoped>
.wrap { max-width: 420px; margin: 0 auto; padding: 56px 26px; display: flex; flex-direction: column; gap: var(--tt-3); min-height: 100%; }
.brand { display: inline-flex; align-items: center; gap: 10px; font-family: var(--tt-font-mono); font-weight: 700; letter-spacing: .12em; color: var(--tt-text); }
.brand .dot { width: 13px; height: 13px; border-radius: 50%; background: var(--tt-accent); box-shadow: 0 0 12px var(--tt-accent); }
h1 { font-size: 34px; font-weight: 800; letter-spacing: -.03em; margin: 24px 0 0; }
p { color: var(--tt-text-muted); margin: 0; line-height: 1.55; }
label { font-family: var(--tt-font-mono); font-size: 12px; letter-spacing: .08em; color: var(--tt-text-faint); margin-top: var(--tt-4); }
.field { height: 56px; border-radius: var(--tt-r-md); background: var(--tt-surface-2); border: 1.5px solid var(--tt-border); color: var(--tt-text); padding: 0 18px; font-size: 18px; font-weight: 600; }
.field:focus { outline: none; border-color: var(--tt-accent); }
.link { background: none; border: none; color: var(--tt-text-muted); margin-top: var(--tt-4); cursor: pointer; }
.err { color: var(--tt-danger); }
</style>
