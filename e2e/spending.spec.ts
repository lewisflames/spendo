import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const BASE: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 4,
      totalTransactions: 3,
      earliestDate: '2024-01-05',
      latestDate: '2024-02-20',
      netWorth: 1500
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 }
};

const category = (
  accountGuid: string,
  accountName: string,
  monthly: number[],
  extra: Record<string, unknown> = {}
) => ({
  accountGuid,
  accountName,
  hasChildren: false,
  ownSplits: false,
  monthly,
  total: monthly.reduce((a, b) => a + b, 0),
  previousTotal: 0,
  changePct: 100,
  ...extra
});

const report = (overrides: Record<string, unknown> = {}) => ({
  dateFrom: '2024-01-01',
  dateTo: '2024-02-29',
  prevDateFrom: '2023-11-01',
  prevDateTo: '2023-12-31',
  reportCurrency: 'EUR',
  path: [],
  months: ['2024-01', '2024-02'],
  categories: [
    category('rent', 'Rent', [800, 0]),
    category('food', 'Food', [120, 30], { hasChildren: true, previousTotal: 90, changePct: 66.7 })
  ],
  total: 950,
  totalPrev: 90,
  changePct: 955.6,
  missingFx: [],
  ...overrides
});

test('lists categories with a drill-down for parents and a ledger link for leaves', async ({
  page
}) => {
  await mockBackend(page, { ...BASE, fetch_spending_report: report() });
  await page.goto('/reports/spending?from=2024-01-01&to=2024-02-29');

  await expect(page.getByRole('heading', { name: 'Spending' })).toBeVisible();
  await expect(page.getByText('Spending by month')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Food' })).toHaveAttribute(
    'href',
    '/reports/spending?category=food&from=2024-01-01&to=2024-02-29'
  );
  await expect(page.getByRole('link', { name: 'Rent' })).toHaveAttribute(
    'href',
    '/accounts/rent?from=2024-01-01&to=2024-02-29'
  );
});

test('titles a drilled-down level after its category with a way back up', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_spending_report: report({
      path: [{ accountGuid: 'food', accountName: 'Food' }],
      categories: [
        category('groceries', 'Groceries', [120, -20]),
        category('food', 'Food', [0, 10], { ownSplits: true })
      ],
      total: 110
    })
  });
  await page.goto('/reports/spending?category=food&from=2024-01-01&to=2024-02-29');

  await expect(page.getByRole('heading', { name: 'Food' })).toBeVisible();
  await expect(
    page.getByRole('navigation', { name: 'Breadcrumb' }).getByRole('link', { name: 'Spending' })
  ).toBeVisible();
  await expect(page.getByRole('link', { name: 'Food (direct)' })).toBeVisible();
});

test('shows an empty state when nothing was spent', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_spending_report: report({ categories: [], total: 0, totalPrev: 0, changePct: null })
  });
  await page.goto('/reports/spending?from=2024-01-01&to=2024-02-29');

  await expect(page.getByText('Nothing spent here')).toBeVisible();
});
