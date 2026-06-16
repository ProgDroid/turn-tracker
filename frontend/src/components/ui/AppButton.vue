<script setup lang="ts">
defineProps<{ variant?: 'accent' | 'ghost'; disabled?: boolean }>()
</script>

<template>
  <button class="btn" :class="variant ?? 'accent'" :disabled="disabled">
    <slot />
  </button>
</template>

<style scoped>
.btn {
  width: 100%; min-height: 56px;
  border: none; border-radius: var(--tt-r-md);
  font-family: var(--tt-font-sans); font-weight: 700; font-size: 18px;
  cursor: pointer;
}
.accent { background: var(--tt-accent); color: var(--tt-on-accent); box-shadow: 0 0 24px -4px var(--tt-accent-glow); }
.ghost { background: transparent; color: var(--tt-text-muted); border: 1.5px solid var(--tt-border); }
.btn:disabled { background: var(--tt-surface-1); color: var(--tt-text-faint); box-shadow: none; cursor: default; }
</style>
