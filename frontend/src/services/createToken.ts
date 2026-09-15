/**
 * Closed-beta host token. Hosts are given a link of the form
 * `https://whosego.app/?k=<token>`; the token is captured once, persisted per
 * device, and sent as a header when creating a room. Joining is never gated.
 */
const KEY = 'tt:createToken'
const PARAM = 'k'

/**
 * Capture `?k=` from the current URL, persist it, and strip it from the
 * address bar so it is not shared along with a copied link.
 */
export function captureCreateTokenFromUrl(): void {
  const url = new URL(window.location.href)
  const token = url.searchParams.get(PARAM)
  if (!token) return
  try {
    localStorage.setItem(KEY, token)
  } catch {
    // Private mode or blocked storage: the header is simply not sent.
  }
  url.searchParams.delete(PARAM)
  window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`)
}

/** The stored host token, if this device has one. */
export function loadCreateToken(): string | null {
  try {
    return localStorage.getItem(KEY)
  } catch {
    return null
  }
}
