<script setup lang="ts">
const props = defineProps<{ name: string; accent?: boolean; size?: number }>()
const initial = () => props.name.trim().charAt(0).toUpperCase() || '?'
</script>

<template>
  <span class="avatar" :class="{ accent }" :style="{ width: (size ?? 34) + 'px', height: (size ?? 34) + 'px' }">
    {{ initial() }}
  </span>
</template>

<style scoped>
.avatar {
  display: inline-flex; align-items: center; justify-content: center;
  border-radius: 50%; background: var(--tt-surface-3); color: var(--tt-text-muted);
  font-weight: 800; font-size: 15px;
}
.avatar.accent { background: var(--tt-accent); color: var(--tt-on-accent); }
</style>
