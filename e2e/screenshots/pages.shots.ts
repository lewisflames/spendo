import { expect, mockBackend, test } from '../fixtures';
import { DEMO_BOOK, TODAY, transactionsFor } from './demo-book';

/** Pages captured for the README and the landing page, in display order. */
const SHOTS = [
  { name: 'overview', path: '/', pageSize: 8 },
  { name: 'transactions', path: '/transactions', pageSize: 25 },
  { name: 'spending', path: '/reports/spending', pageSize: 25 },
  { name: 'budgets', path: '/budgets', pageSize: 25 },
  { name: 'portfolio', path: '/portfolio', pageSize: 25 }
];

for (const theme of ['light', 'dark'] as const) {
  for (const shot of SHOTS) {
    test(`${shot.name} (${theme})`, async ({ page }) => {
      await page.clock.setFixedTime(new Date(`${TODAY}T10:30:00`));
      await page.addInitScript((theme) => {
        localStorage.setItem('spendo-settings', JSON.stringify({ theme }));
      }, theme);
      await mockBackend(page, {
        ...DEMO_BOOK,
        fetch_transactions: transactionsFor(shot.pageSize)
      });

      await page.goto(shot.path);
      await page.waitForLoadState('networkidle');
      // Charts animate in; wait for the last frame.
      await page.waitForTimeout(1200);
      await expect(page.getByText(/unmocked command/)).toHaveCount(0);

      await page.screenshot({ path: `docs/screenshots/${shot.name}-${theme}.png` });
    });
  }
}
