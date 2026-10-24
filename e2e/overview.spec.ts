import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const point = (date: string, assets: number, liabilities: number) => ({
  date,
  assets,
  liabilities,
  netWorth: assets - liabilities
});

const OVERVIEW: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 3,
      totalTransactions: 2,
      earliestDate: '2024-01-10',
      latestDate: '2024-02-05',
      netWorth: 5
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_pnl_report: {
    rows: [],
    totalIncome: 0,
    totalExpenses: 0,
    netIncome: 0,
    prevTotalIncome: 0,
    prevTotalExpenses: 0,
    prevNetIncome: 0,
    missingFx: []
  },
  fetch_pnl_trend: [],
  fetch_transactions: { transactions: [], total: 0, page: 1, pageSize: 8, totalPages: 1 }
};

test('shows the net worth chart with the full range by default', async ({ page }) => {
  await mockBackend(page, {
    ...OVERVIEW,
    fetch_net_worth_trend: [point('2024-01-31', 60, 0), point('2024-02-29', 45, 40)]
  });
  await page.goto('/');

  await expect(page.getByText('Net worth over time')).toBeVisible();
  const ranges = page.getByRole('group', { name: 'Chart date range' });
  await expect(ranges.getByRole('button')).toHaveText(['1Y', '5Y', 'All']);
  await expect(ranges.getByRole('button', { name: 'Show all dates' })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
});

test('shows an empty state before the first transaction', async ({ page }) => {
  await mockBackend(page, { ...OVERVIEW, fetch_net_worth_trend: [] });
  await page.goto('/');

  await expect(
    page.getByText('Your net worth story starts with the first transaction.')
  ).toBeVisible();
});

test('offers a retry when the trend fails to load', async ({ page }) => {
  await mockBackend(page, OVERVIEW);
  await page.goto('/');

  await expect(page.getByText('unmocked command: fetch_net_worth_trend')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible();
  await expect(page.getByText('Net worth over time')).toHaveCount(0);
});
