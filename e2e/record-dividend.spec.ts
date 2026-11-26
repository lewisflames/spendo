import { accountsResult, type BackendMocks, expect, mockBackend, test } from './fixtures';
import { ZERO_UNITS } from '../src/lib/utils/units';

const account = (guid: string, name: string, accountType: string, commodityGuid = 'eur') => ({
  guid,
  name,
  accountType,
  code: null,
  description: null,
  placeholder: false,
  commodity: commodityGuid === 'eur' ? 'EUR' : 'AAPL',
  commodityGuid,
  balance: ZERO_UNITS,
  txCount: 0,
  children: []
});

const MOCKS: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 3,
      totalTransactions: 1,
      earliestDate: '2024-01-10',
      latestDate: '2024-01-10',
      netWorth: 1000,
      readOnly: false
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_accounts: accountsResult({
    trees: [
      account('broker', 'Broker', 'STOCK', 'c-aapl'),
      account('div', 'Dividends', 'INCOME'),
      account('bank', 'Bank', 'BANK')
    ],
    byType: {},
    totalAccounts: 3,
    reportCurrency: 'EUR',
    retainedEarnings: ZERO_UNITS,
    unrealizedGains: ZERO_UNITS
  }),
  fetch_holding_detail: {
    holding: {
      commodityGuid: 'c-aapl',
      mnemonic: 'AAPL',
      fullName: null,
      namespace: 'NASDAQ',
      shares: 10,
      costBasis: 1000,
      openCostBasis: 1000,
      marketValue: 1000,
      avgCostPerShare: 100,
      gainLoss: 0,
      unrealizedGain: 0,
      realizedGain: 0,
      dividends: 0,
      totalReturn: 0,
      basisWarning: null,
      xirr: null,
      allocationPct: 100,
      price: 100,
      prevPrice: null,
      priceDate: '2024-01-10',
      accounts: [
        {
          accountGuid: 'broker',
          accountName: 'Broker',
          accountDescription: null,
          accountCode: null,
          shares: 10,
          costBasis: 1000
        }
      ]
    },
    dividendHistory: [],
    dividendAccounts: [{ accountGuid: 'div', accountName: 'Dividends' }],
    openLots: [],
    priceHistory: [],
    valueTrend: [],
    transactions: [],
    total: 0,
    page: 1,
    pageSize: 25,
    totalPages: 1
  }
};

test('prefills the zero split and the linked income account', async ({ page }) => {
  await mockBackend(page, MOCKS);
  await page.goto('/transactions/new?dividend=c-aapl');

  await expect(page.getByRole('heading', { name: 'Record dividend' })).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Description' })).toHaveValue('AAPL dividend');
  // Splits are labelled by their account once one is picked.
  await expect(page.getByRole('textbox', { name: 'Broker amount' })).toHaveValue('0');
  await expect(page.getByRole('textbox', { name: 'Broker memo' })).toHaveValue('Dividend');
  await expect(page.getByRole('textbox', { name: 'Dividends amount' })).toHaveValue('');
  await expect(page.getByRole('textbox', { name: 'Split 3 amount' })).toHaveValue('');
});
