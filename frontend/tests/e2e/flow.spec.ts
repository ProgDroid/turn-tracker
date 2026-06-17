import { test, expect } from '@playwright/test'
import { newPlayer, createRoom, joinRoom } from './helpers'

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
