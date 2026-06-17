import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 30_000,
  use: { baseURL: 'http://127.0.0.1:8080', trace: 'on-first-retry' },
  webServer: [
    {
      // Build the SPA, then run the Rust backend serving it.
      command:
        'npm run build && cross-env STATIC_DIR=./dist BIND_ADDR=127.0.0.1:8080 cargo run --manifest-path ../Cargo.toml --release',
      url: 'http://127.0.0.1:8080',
      reuseExistingServer: !process.env.CI,
      timeout: 180_000,
      cwd: '.',
    },
  ],
})
