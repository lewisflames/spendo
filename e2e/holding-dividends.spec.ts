import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const HOLDING = {
  commodityGuid: 'c-aapl',
  mnemonic: 'AAPL',
  fullName: 'Apple Inc',
  namespace: 'NASDAQ',
  shares: 10,
  costBasis: 1000,
  openCostBasis: 1000,
  marketValue: 1500,
  avgCostPerShare: 100,
  gainLoss: 500,
  unrealizedGain: 500,
  realizedGain: 0,
  dividends: 34,
  totalReturn: 534,
  basisWarning: null,
  xirr: 12.5,
  allocationPct: 100,
  price: 150,
  prevPrice: 149,
  priceDate: '2024-06-01',
  accounts: []
};

const MOCKS: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 3,
      totalTransactions: 2,
      earliestDate: '2024-01-10',
      latestDate: '2024-03-01',
      netWorth: 1500
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_symbol_overrides: [],
  fetch_holding_detail: {
    holding: HOLDING,
    dividendHistory: [
      { txGuid: 't-d1', postDate: '2024-03-01', description: 'AAPL dividend', amount: 34 }
    ],
    dividendAccounts: [{ accountGuid: 'a-div', accountName: 'Dividends' }],
    openLots: [
      { acquired: '2024-01-10', shares: 10, basis: 1000, marketValue: 1500, unrealizedGain: 500 }
    ],
    priceHistory: [],
    valueTrend: [],
    transactions: [],
    total: 0,
    page: 1,
    pageSize: 25,
    totalPages: 1
  },
  fetch_accounts: { trees: [] }
};

test('lists dividends and the linked income accounts', async ({ page }) => {
  await mockBackend(page, MOCKS);
  await page.goto('/portfolio/c-aapl');

  await expect(page.getByText('All-time realized gain', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Dividends' }).click();
  await expect(page.getByRole('link', { name: 'AAPL dividend' })).toBeVisible();

  await page.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('menuitem', { name: 'Dividend accounts' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Dividend accounts for AAPL')).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Unlink Dividends' })).toBeVisible();
});

test('shows open lots with their unrealized gain', async ({ page }) => {
  await mockBackend(page, MOCKS);
  await page.goto('/portfolio/c-aapl');

  await page.getByRole('tab', { name: 'Lots' }).click();
  const lotsPanel = page.getByRole('tabpanel');
  await expect(lotsPanel.getByRole('columnheader', { name: 'Cost per share' })).toBeVisible();
  await expect(lotsPanel.getByRole('row')).toHaveCount(2);
});
