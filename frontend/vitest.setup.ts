// jsdom 29 under vitest 4 exposes a localStorage whose clear() is not callable,
// so install a spec-correct in-memory Storage. Vitest isolates each test file in
// a fresh environment (isolate: true is the default), so this backing map is
// recreated per file and does not leak state across files.
class MemoryStorage {
  private m = new Map<string, string>()
  get length(): number {
    return this.m.size
  }
  clear(): void {
    this.m.clear()
  }
  getItem(key: string): string | null {
    return this.m.has(key) ? (this.m.get(key) as string) : null
  }
  setItem(key: string, value: string): void {
    this.m.set(key, String(value))
  }
  removeItem(key: string): void {
    this.m.delete(key)
  }
  key(index: number): string | null {
    return Array.from(this.m.keys())[index] ?? null
  }
}

Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  value: new MemoryStorage() as unknown as Storage,
})
