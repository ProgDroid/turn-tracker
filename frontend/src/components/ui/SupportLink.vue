<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

/**
 * Ko-fi tip jar. Deliberately a plain anchor with an INLINE icon rather than
 * Ko-fi's official button: that is a third-party <script> embed, and this app
 * ships `script-src 'self'` / `img-src 'self' data:`. Using it would mean
 * loosening the CSP for a tip jar. Links themselves are not restricted by those
 * directives, so this costs nothing.
 *
 * Points at the personal page rather than a project-specific one so support
 * accumulates in one place across projects. Swapping it is this one constant.
 */
const KOFI_URL = 'https://ko-fi.com/progdroid1984'
</script>

<template>
  <a
    class="support"
    :href="KOFI_URL"
    target="_blank"
    rel="noopener noreferrer"
    :aria-label="t('support.kofiAria')"
  >
    <svg class="ico" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" focusable="false">
      <path
        d="M4 4h13a4 4 0 0 1 0 8h-1M4 4v9a4 4 0 0 0 4 4h4a4 4 0 0 0 4-4V4M3 21h14"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
    {{ t('support.kofi') }}
  </a>
</template>

<style scoped>
.support {
  display: inline-flex; align-items: center; gap: 7px;
  align-self: center;
  /* Reads as chrome, not a call to action: faint, small, no button affordance. */
  font-size: 12px; color: var(--tt-text-faint);
  text-decoration: none;
  padding: 10px 8px;          /* tap target, not visual weight */
  transition: color .15s ease;
}
.support:hover, .support:focus-visible { color: var(--tt-text-muted); }
.support:focus-visible { outline: 2px solid var(--tt-accent); outline-offset: 2px; border-radius: var(--tt-r-sm, 6px); }
.ico { flex-shrink: 0; }
@media (prefers-reduced-motion: reduce) { .support { transition: none; } }
</style>
