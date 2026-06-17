import { ref, onMounted, onUnmounted } from 'vue'

export function useReducedMotion() {
  const reduced = ref(false)
  let mq: MediaQueryList | null = null
  const update = () => { reduced.value = !!mq?.matches }
  onMounted(() => {
    mq = window.matchMedia('(prefers-reduced-motion: reduce)')
    update()
    mq.addEventListener('change', update)
  })
  onUnmounted(() => mq?.removeEventListener('change', update))
  return { reduced }
}
