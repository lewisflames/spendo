import { type BackendMocks, expect, mockBackend, pnlReport, test } from './fixtures';

const summary = (readOnly: boolean) => ({
  currency: 'EUR',
  totalAccounts: 3,
  totalTransactions: 0,
  earliestDate: null,
  latestDate: null,
  netWorth: 0,
  readOnly
});

const BOOK: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_pnl_report: pnlReport(),
  fetch_pnl_trend: [],
  fetch_net_worth_trend: [],
  fetch_transactions: { transactions: [], total: 0, page: 1, pageSize: 8, totalPages: 1 },
  post_due_schedules: { posted: 0, failed: [] }
};

const BANNER = 'This book was saved by a newer Spendo. Update to edit it';

test('a book from a newer Spendo shows the read-only banner', async ({ page }) => {
  await mockBackend(page, {
    ...BOOK,
    fetch_home_data: { hasData: true, summary: summary(true) }
  });
  await page.goto('/');

  await expect(page.getByRole('status').filter({ hasText: BANNER })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Check for updates' })).toHaveAttribute(
    'href',
    '/about'
  );
});

test('a writable book shows no banner', async ({ page }) => {
  await mockBackend(page, {
    ...BOOK,
    fetch_home_data: { hasData: true, summary: summary(false) }
  });
  await page.goto('/');

  await expect(page.getByText('Net worth over time')).toBeVisible();
  await expect(page.getByText(BANNER)).toHaveCount(0);
});
