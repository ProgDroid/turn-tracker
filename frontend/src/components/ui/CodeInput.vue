<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps<{ modelValue?: string }>()
const emit = defineEmits<{
  (e: 'update:modelValue', v: string): void
  (e: 'complete', v: string): void
}>()

const value = ref(props.modelValue ?? '')

function onInput(e: Event) {
  const raw = (e.target as HTMLInputElement).value.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 6)
  value.value = raw
  emit('update:modelValue', raw)
  if (raw.length === 6) emit('complete', raw)
}

watch(() => props.modelValue, (v) => { if (v !== undefined) value.value = v })
</script>

<template>
  <input
    class="code-input"
    inputmode="text"
    autocapitalize="characters"
    autocomplete="off"
    maxlength="6"
    :value="value"
    @input="onInput"
  />
</template>

<style scoped>
.code-input {
  width: 100%;
  font-family: var(--tt-font-mono);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  background: var(--tt-surface-2);
  border: 1.5px solid var(--tt-border);
  border-radius: var(--tt-r-md);
  color: var(--tt-text);
  padding: var(--tt-4);
  min-height: 56px;
  font-size: 24px;
  text-align: center;
}
.code-input:focus { outline: none; border-color: var(--tt-accent); }
</style>
