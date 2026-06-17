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

export async function joinRoom(page: Page, code: string, name: string) {
  await page.goto(`/room/${code}?name=${encodeURIComponent(name)}`)
}
