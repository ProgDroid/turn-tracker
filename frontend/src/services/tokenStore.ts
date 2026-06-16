const key = (code: string) => `tt:token:${code.toUpperCase()}`

export function saveToken(code: string, token: string): void {
  localStorage.setItem(key(code), token)
}

export function loadToken(code: string): string | null {
  return localStorage.getItem(key(code))
}

export function clearToken(code: string): void {
  localStorage.removeItem(key(code))
}
