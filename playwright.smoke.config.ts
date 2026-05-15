import { defineConfig } from '@playwright/test'

const baseURL = process.env.BASE_URL || 'http://127.0.0.1:41741'
const port = new URL(baseURL).port || '41741'
const reuseExistingServer = process.env.PLAYWRIGHT_REUSE_SERVER
  ? process.env.PLAYWRIGHT_REUSE_SERVER === '1'
  : process.env.CI !== 'true'
const claudeCodeOnboardingStorageState = {
  cookies: [],
  origins: [
    {
      origin: baseURL,
      localStorage: [
        { name: 'tolaria:claude-code-onboarding-dismissed', value: '1' },
      ],
    },
  ],
}

export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  /**
   * Hard ceiling on the entire smoke run. Per AGENTS.md, smoke should stay
   * under 5 min; we cap at 10 min so a few cascading first-try flakes can't
   * compound to hours like they did on 2026-05-14 (one run hit ~2h because
   * seven tests each hung for ~15 min before passing on retry, even though
   * `timeout: 30_000` should have killed them sooner). Hitting the ceiling
   * fails the suite — which is the right signal to fix the underlying flake
   * instead of letting it ride.
   */
  globalTimeout: 600_000,
  retries: 1,
  workers: 1,
  grep: /@smoke/,
  use: {
    baseURL,
    headless: true,
    storageState: claudeCodeOnboardingStorageState,
  },
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
  webServer: {
    command: `node scripts/playwright-smoke-server.mjs ${port}`,
    url: baseURL,
    reuseExistingServer,
    timeout: 30_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
})
