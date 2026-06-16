import { describe, it, expect } from 'vitest'
import { router } from '@/router'

describe('router', () => {
  it('has landing and room routes', () => {
    const names = router.getRoutes().map((r) => r.path)
    expect(names).toContain('/')
    expect(names).toContain('/room/:code')
  })
})
