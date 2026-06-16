<script setup lang="ts">
defineProps<{ tone?: 'info' | 'danger' | 'warning' | 'success' }>()
</script>

<template>
  <div class="toast" :class="tone ?? 'info'" role="status">
    <slot />
  </div>
</template>

<style scoped>
.toast {
  display: flex; align-items: center; gap: var(--tt-3);
  border-radius: 13px; padding: 14px 16px; font-size: 14px; font-weight: 700;
  background: var(--tt-surface-2); border: 1px solid var(--tt-border);
}
.info { color: var(--tt-info); border-color: rgba(56, 189, 248, 0.28); }
.danger { color: var(--tt-danger); border-color: rgba(251, 113, 133, 0.28); }
.warning { color: var(--tt-warning); border-color: rgba(251, 191, 36, 0.28); }
.success { color: var(--tt-accent); border-color: rgba(52, 211, 153, 0.28); }
</style>
