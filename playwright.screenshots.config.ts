import { defineConfig, devices } from '@playwright/test';
import base from './playwright.config';

// `bun run screenshots`: renders the demo book (e2e/screenshots/demo-book.ts)
// into docs/screenshots/ for the README and the landing page.
export default defineConfig({
  ...base,
  testDir: 'e2e/screenshots',
  testMatch: '*.shots.ts',
  retries: 0,
  reporter: 'list',
  use: {
    ...base.use,
    ...devices['Desktop Chrome'],
    viewport: { width: 1280, height: 880 },
    deviceScaleFactor: 2
  },
  projects: undefined
});
