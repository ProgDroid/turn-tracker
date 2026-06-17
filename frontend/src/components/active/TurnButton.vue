<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const props = defineProps<{ disabled?: boolean }>()
const emit = defineEmits<{ (e: 'done'): void }>()
const { t } = useI18n()
</script>

<template>
  <button class="turn" :class="{ disabled }" :disabled="disabled" @click="emit('done')">
    <span class="sheen" v-if="!disabled" aria-hidden="true" />
    <span class="label">{{ t('active.done') }}</span>
  </button>
</template>

<style scoped>
.turn {
  width: 100%; min-height: 130px; border: none; cursor: pointer;
  border-radius: var(--tt-r-xl); background: var(--tt-on-accent);
  color: var(--tt-accent); font-size: 40px; font-weight: 800; letter-spacing: .04em;
  position: relative; overflow: hidden; box-shadow: inset 0 -5px 0 rgba(0, 0, 0, .45);
}
.turn.disabled { background: var(--tt-surface-1); color: var(--tt-text-faint); box-shadow: none; cursor: default; }
.turn:active:not(.disabled) { transform: scale(.985); background: #021f12; color: var(--tt-accent-strong); }
.sheen { position: absolute; top: 0; bottom: 0; width: 60px; background: linear-gradient(90deg, transparent, rgba(52,211,153,.22), transparent); animation: ttSheen 3.2s ease-in-out infinite; }
.label { position: relative; }
@media (prefers-reduced-motion: reduce) { .sheen { animation: none; } }
</style>
