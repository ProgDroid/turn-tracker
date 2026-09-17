import type { Page, BrowserContext, Browser } from '@playwright/test'

/** Stub device APIs so headless Chromium doesn't throw and we can spy. */
export async function stubDeviceApis(context: BrowserContext) {
  await context.addInitScript(() => {
    ;(window as any).__calls = { vibrate: 0, wakeLock: 0 }
    ;(navigator as any).vibrate = () => { ;(window as any).__calls.vibrate++; return true }
    ;(navigator as any).wakeLock = { request: async () => { ;(window as any).__calls.wakeLock++; return { release: async () => {} } } }
  })
}

export async function newPlayer(browser: Browser) {
  const context = await browser.newContext()
  await stubDeviceApis(context)
  const page = await context.newPage()
  return { context, page }
}

export async function createRoom(page: Page, name: string): Promise<string> {
  await page.goto('/')
  await page.getByTestId('name').fill(name)
  await page.getByTestId('create').click()
  await page.waitForURL(/\/room\/[A-Z0-9]{6}/)
  const m = page.url().match(/\/room\/([A-Z0-9]{6})/)
  return m![1]
}

/**
 * Join as a newcomer arriving on a shared link: no saved token and no name
 * handed over in `history.state`, so RoomView shows its name gate. (A `?name=`
 * query is NOT read by RoomView — passing one just left the joiner sitting on
 * the gate, never joining.)
 */
export async function joinRoom(page: Page, code: string, name: string) {
  await page.goto(`/room/${code}`)
  await page.locator('[data-test="join-name"]').fill(name)
  await page.locator('[data-test="join-submit"]').click()
}

/**
 * Whole seconds on the current turn, read from the clock's `datetime`
 * (`PT<n>S`) rather than its text, which also carries screen-reader context.
 * Throws if the screen shows no clock, so a silently-missing readout fails
 * loudly instead of passing as zero.
 */
export async function turnClockSecs(page: Page): Promise<number> {
  const dt = await page.locator('[data-test="turn-clock"]').getAttribute('datetime')
  const m = /^PT(\d+)S$/.exec(dt ?? '')
  if (!m) throw new Error(`no turn clock on this screen (datetime=${dt})`)
  return Number(m[1])
}

