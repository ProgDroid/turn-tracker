import { test, expect } from '@playwright/test'
import { newPlayer, createRoom, joinRoom, turnClockSecs } from './helpers'

test('multi-player turn flow propagates across clients', async ({ browser }) => {
  const host = await newPlayer(browser)
  const bob = await newPlayer(browser)
  const cara = await newPlayer(browser)

  const code = await createRoom(host.page, 'Sam')
  await joinRoom(bob.page, code, 'Bob')
  await joinRoom(cara.page, code, 'Cara')

  // Host lobby shows 3 players.
  await expect(host.page.getByText('Players · 3')).toBeVisible()

  // Start the game.
  await host.page.getByText('Start game').click()

  // Host is first (created first) → host sees YOUR TURN.
  await expect(host.page.getByText("It's your turn")).toBeVisible()
  // Bob sees Sam's turn (watching UI) or a claim-turn heading — either way a heading mentioning turn.
  await expect(bob.page.getByRole('heading', { name: /turn/i })).toBeVisible()

  // Host taps Done → turn advances to the next player.
  await host.page.getByRole('button', { name: 'DONE' }).click()
  await expect(host.page.getByText("It's your turn")).toBeHidden()

  // Vibrate was called on the host when their turn began.
  const vib = await host.page.evaluate(() => (window as any).__calls.vibrate)
  expect(vib).toBeGreaterThan(0)
})

test('non-current player sees watching UI and not their-turn indicator', async ({ browser }) => {
  const host = await newPlayer(browser)
  const bob = await newPlayer(browser)
  const code = await createRoom(host.page, 'Sam')
  await joinRoom(bob.page, code, 'Bob')

  await host.page.getByText('Players · 2').waitFor()
  await host.page.getByText('Start game').click()

  // Sam is current — Bob is watching.
  // Bob must see a turn-related heading (e.g. "Sam is finishing their turn").
  await expect(bob.page.getByRole('heading', { name: /turn/i })).toBeVisible()
  // Bob must NOT see the active-player prompt.
  await expect(bob.page.getByText("It's your turn")).toBeHidden()
})

test('reconnect restores the player via stored token', async ({ browser }) => {
  const host = await newPlayer(browser)
  const code = await createRoom(host.page, 'Sam')
  await expect(host.page.getByText('Players · 1')).toBeVisible()

  // Reload (new page load reuses localStorage token in the same context).
  await host.page.reload()
  await expect(host.page).toHaveURL(`http://127.0.0.1:8080/room/${code}`)
  await expect(host.page.getByText('Players · 1')).toBeVisible()
})

test('the turn clock agrees across clients, including one that joined mid-turn', async ({ browser }) => {
  const host = await newPlayer(browser)
  const bob = await newPlayer(browser)
  const code = await createRoom(host.page, 'Sam')
  await joinRoom(bob.page, code, 'Bob')
  await expect(host.page.getByText('Players · 2')).toBeVisible()

  await host.page.getByText('Start game').click()
  await expect(host.page.getByText("It's your turn")).toBeVisible()

  // Let the turn actually run. The elapsed time IS the thing under test here,
  // so waiting it out is the measurement, not a stand-in for a condition.
  await host.page.waitForTimeout(3_000)

  // Cara arrives after the turn began. A client-local stopwatch would start
  // her at 0:00 while everyone else reads ~3 — the case that makes the count
  // worth resolving on the server.
  const cara = await newPlayer(browser)
  await joinRoom(cara.page, code, 'Cara')
  await expect(cara.page.locator('[data-test="turn-clock"]')).toBeVisible()

  const secs = [
    await turnClockSecs(host.page),
    await turnClockSecs(bob.page),
    await turnClockSecs(cara.page),
  ]
  expect(secs[2]).toBeGreaterThanOrEqual(2)
  // A second of spread is the tick boundary; more means they genuinely disagree.
  expect(Math.max(...secs) - Math.min(...secs)).toBeLessThanOrEqual(1)
})

test('the turn clock restarts for everyone when the turn changes hands', async ({ browser }) => {
  const host = await newPlayer(browser)
  const bob = await newPlayer(browser)
  const code = await createRoom(host.page, 'Sam')
  await joinRoom(bob.page, code, 'Bob')
  await expect(host.page.getByText('Players · 2')).toBeVisible()

  await host.page.getByText('Start game').click()
  await expect(host.page.getByText("It's your turn")).toBeVisible()
  await host.page.waitForTimeout(3_000)
  expect(await turnClockSecs(host.page)).toBeGreaterThanOrEqual(2)

  await host.page.getByRole('button', { name: 'DONE' }).click()
  await expect(bob.page.getByText("It's your turn")).toBeVisible()

  // Both ends of the handover see a fresh clock, not a continuing one.
  expect(await turnClockSecs(bob.page)).toBeLessThanOrEqual(1)
  expect(await turnClockSecs(host.page)).toBeLessThanOrEqual(1)
})

