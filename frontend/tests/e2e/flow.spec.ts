import { test, expect } from '@playwright/test'
import { newPlayer, createRoom, joinRoom, turnClockSecs, lobbyOrder } from './helpers'

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

test('a host shuffle randomises the order and reaches every client', async ({ browser }) => {
  const host = await newPlayer(browser)
  const others = [await newPlayer(browser), await newPlayer(browser), await newPlayer(browser)]
  const names = ['Bob', 'Cara', 'Dan']

  const code = await createRoom(host.page, 'Sam')
  for (let i = 0; i < others.length; i++) await joinRoom(others[i].page, code, names[i])
  await expect(host.page.getByText('Players · 4')).toBeVisible()

  const before = await lobbyOrder(host.page)
  expect(before).toEqual(['Sam', 'Bob', 'Cara', 'Dan'])

  // A fair shuffle can land back on the order it started from — 1 in 24 for
  // four players — so asserting that a single click changed something would
  // fail roughly 4% of runs. Shuffle until it differs instead; six attempts
  // makes an all-identity run (1/24^6) not worth worrying about, and a shuffle
  // that never randomises still fails.
  let after = before
  for (let i = 0; i < 6 && after.join() === before.join(); i++) {
    await host.page.locator('[data-test="shuffle"]').click()
    await host.page.waitForTimeout(250)
    after = await lobbyOrder(host.page)
  }
  expect(after).not.toEqual(before)
  // A permutation, not a rewrite: nobody gained, lost or renamed.
  expect([...after].sort()).toEqual([...before].sort())

  // The reason it rides `set_order` rather than staying local: every client
  // ends up on the same order, not just the host who pressed the button.
  for (const p of others) {
    await expect.poll(() => lobbyOrder(p.page)).toEqual(after)
  }
})

test('a lone host closing the room lands back on the landing page', async ({ browser }) => {
  const host = await newPlayer(browser)
  await createRoom(host.page, 'Sam')
  await expect(host.page.getByText('Players · 1')).toBeVisible()
  await host.page.getByText('Start game').click()
  await expect(host.page.getByText("It's your turn")).toBeVisible()

  await host.page.getByRole('button', { name: /Host/ }).click()
  // Alone, the action says what it does rather than "Remove from room".
  await expect(host.page.getByText('Close room')).toBeVisible()
  await host.page.locator('[data-test="remove"]').click()

  // Home, promptly — not a turn with nobody in it, and not 23s of reconnecting.
  await expect(host.page).toHaveURL('http://127.0.0.1:8080/')
  await expect(host.page.getByTestId('create')).toBeVisible()

  // That the code itself stops resolving is asserted server-side, in
  // `test_last_player_leaving_closes_the_room_and_frees_the_code`. Repeating it
  // here would cost the client's full ~23s reconnect cap for no new coverage.
})

