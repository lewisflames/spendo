import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const BASE: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 3,
      totalTransactions: 2,
      earliestDate: '2023-01-10',
      latestDate: '2024-03-10',
      netWorth: 1500
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 }
};

const sale = (acquired: string | null, gain: number) => ({
  date: '2024-03-10',
  commodityGuid: 'c-aapl',
  mnemonic: 'AAPL',
  fullName: 'Apple Inc',
  acquired,
  shares: 5,
  proceeds: 750,
  basis: 750 - gain,
  gain
});

test('lists each lot a sale consumed with yearly totals', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_realized_gains: {
      year: 2024,
      rows: [sale('2023-01-10', 250), sale(null, -100)],
      totalProceeds: 1500,
      totalBasis: 1350,
      totalGains: 250,
      totalLosses: -100,
      netGain: 150,
      dividends: 20,
      years: [2023, 2024]
    }
  });
  await page.goto('/reports/gains?year=2024');

  await expect(page.getByRole('heading', { name: 'Realized gains' })).toBeVisible();
  await expect(page.getByRole('link', { name: /Apple Inc/ })).toHaveCount(2);
  await expect(page.getByText('Net realized')).toBeVisible();
  await expect(page.getByRole('button', { name: 'More actions' })).toBeVisible();
});

test('shows an empty state for a year without sales', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_realized_gains: {
      year: 2022,
      rows: [],
      totalProceeds: 0,
      totalBasis: 0,
      totalGains: 0,
      totalLosses: 0,
      netGain: 0,
      dividends: 0,
      years: []
    }
  });
  await page.goto('/reports/gains?year=2022');

  await expect(page.getByText('No sales in 2022')).toBeVisible();
});
