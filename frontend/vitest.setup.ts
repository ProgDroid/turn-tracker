// jsdom provides localStorage, but it might be incomplete
// Ensure all localStorage methods are available
const createLocalStorage = () => {
  let store: Record<string, string> = {}
  return {
    getItem: (key: string) => store[key] || null,
    setItem: (key: string, value: string) => {
      store[key] = value
    },
    removeItem: (key: string) => {
      delete store[key]
    },
    clear: () => {
      store = {}
    },
    key: (index: number) => {
      const keys = Object.keys(store)
      return keys[index] || null
    },
    get length() {
      return Object.keys(store).length
    },
  } as Storage
}

// Replace the global localStorage with our complete implementation
Object.defineProperty(global, 'localStorage', {
  value: createLocalStorage(),
  writable: true,
  configurable: true,
})
