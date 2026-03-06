import { defineConfig } from "@playwright/test"

export default defineConfig({
  testDir: `./tests/e2e`,
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: {
    timeout: 15_000,
  },
  globalSetup: `./tests/e2e/global.setup.ts`,
  use: {
    baseURL: `https://localhost:3000`,
    ignoreHTTPSErrors: true,
    locale: `en-US`,
    timezoneId: `Europe/Berlin`,
    trace: `retain-on-failure`,
    screenshot: `only-on-failure`,
    video: `retain-on-failure`,
    viewport: {
      width: 1440,
      height: 960,
    },
  },
  projects: [
    {
      name: `chromium`,
      use: {
        browserName: `chromium`,
      },
    },
  ],
  webServer: {
    command:
      `DISABLE_TANSTACK_DEVTOOLS=1 ` +
      `DISABLE_ROUTER_DEVTOOLS=1 ` +
      `VITE_DISABLE_ROUTER_DEVTOOLS=1 ` +
      `BETTER_AUTH_URL=https://localhost:3000 ` +
      `BETTER_AUTH_TRUSTED_ORIGINS=https://localhost:3000 ` +
      `bun dev --host 0.0.0.0 --port 5173`,
    port: 5173,
    reuseExistingServer: false,
    timeout: 120_000,
  },
})
