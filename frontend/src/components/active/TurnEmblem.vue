<script setup lang="ts">
defineProps<{ fast?: boolean }>()
</script>

<template>
  <div class="emblem">
    <span class="pulse" :class="{ fast }" aria-hidden="true" />
    <span class="pulse delay" :class="{ fast }" aria-hidden="true" />
    <span class="core">!</span>
  </div>
</template>

<style scoped>
.emblem { position: relative; width: 150px; height: 150px; display: flex; align-items: center; justify-content: center; }
.pulse { position: absolute; width: 150px; height: 150px; border-radius: 50%; background: rgba(4, 20, 13, .16); animation: ttPulse 2.4s ease-out infinite; }
.pulse.delay { animation-delay: 1.2s; }
.pulse.fast { animation-duration: 1.6s; }
.core { position: relative; width: 108px; height: 108px; border-radius: 50%; background: var(--tt-on-accent); color: var(--tt-accent); display: flex; align-items: center; justify-content: center; font-size: 46px; font-weight: 800; }
@media (prefers-reduced-motion: reduce) { .pulse { animation: none; opacity: 0; } }
</style>
