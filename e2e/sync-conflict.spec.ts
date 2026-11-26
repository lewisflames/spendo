import { type BackendMocks, expect, mockBackend, pnlReport, test } from './fixtures';

const COPY = {
  path: '/sync/home.sync-conflict-20261125-143022-ABCDEF7.spendo',
  fileName: 'home.sync-conflict-20261125-143022-ABCDEF7.spendo',
  savedAt: '2026-11-25T14:30:22',
  device: 'ABCDEF7'
};

const tx = (guid: string, description: string, amount: number) => ({
  guid,
  postDate: '2026-11-20',
  description,
  payee: null,
  amount,
  accounts: ['Bank', 'Groceries']
});

const BOOK: BackendMocks = {
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 3,
      totalTransactions: 2,
      earliestDate: null,
      latestDate: null,
      netWorth: 0,
      readOnly: false
    }
  },
  fetch_active_book: { path: '/sync/home.spendo' },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_pnl_report: pnlReport(),
  fetch_pnl_trend: [],
  fetch_net_worth_trend: [],
  fetch_transactions: { transactions: [], total: 0, page: 1, pageSize: 8, totalPages: 1 },
  post_due_schedules: { posted: 0, failed: [] },
  list_sync_conflicts: [COPY],
  review_sync_conflict: {
    copy: COPY,
    onlyInCopy: [tx('t-new', 'Farmers market', 32.5)],
    onlyInBook: [],
    changed: [
      {
        book: tx('t-edit', 'Groceries', 40),
        copy: tx('t-edit', 'Big groceries', 45),
        fields: ['description', 'splits']
      }
    ],
    unchanged: 12
  }
};

test('a conflict copy next to the book shows the banner, review lists the differences', async ({
  page
}) => {
  await mockBackend(page, BOOK);
  await page.goto('/');

  const banner = page.getByRole('status').filter({ hasText: 'A conflicting copy from' });
  await expect(banner).toBeVisible();
  await banner.getByRole('link', { name: 'Review' }).click();

  await expect(page).toHaveURL(/\/sync-conflict\?file=home\.sync-conflict-/);
  await expect(page.getByText('Farmers market')).toBeVisible();
  await expect(page.getByText('Copy: Big groceries')).toBeVisible();
  await expect(page.getByText('Amounts or accounts')).toBeVisible();
  // The banner stays off its own review page.
  await expect(page.getByText('A conflicting copy from')).toHaveCount(0);
});

test('dismissing the banner is remembered for that file', async ({ page }) => {
  await mockBackend(page, BOOK);
  await page.goto('/');

  const banner = page.getByRole('status').filter({ hasText: 'A conflicting copy from' });
  await banner.getByRole('button', { name: 'Dismiss' }).click();
  await expect(banner).toHaveCount(0);

  await page.reload();
  await expect(page.getByText('Net worth over time')).toBeVisible();
  await expect(page.getByText('A conflicting copy from')).toHaveCount(0);
});

test('no conflict copy, no banner', async ({ page }) => {
  await mockBackend(page, { ...BOOK, list_sync_conflicts: [] });
  await page.goto('/');

  await expect(page.getByText('Net worth over time')).toBeVisible();
  await expect(page.getByText('A conflicting copy from')).toHaveCount(0);
});
