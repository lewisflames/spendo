import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const account = (guid: string, name: string, accountType: string, commodityGuid = 'eur') => ({
  guid,
  name,
  accountType,
  code: null,
  description: null,
  placeholder: false,
  commodity: commodityGuid === 'eur' ? 'EUR' : 'AAPL',
  commodityGuid,
  balance: 0,
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
      netWorth: 1000
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_accounts: {
    trees: [
      account('broker', 'Broker', 'STOCK', 'c-aapl'),
      account('div', 'Dividends', 'INCOME'),
      account('bank', 'Bank', 'BANK')
    ],
    byType: {},
    totalAccounts: 3,
    reportCurrency: 'EUR',
    retainedEarnings: 0,
    unrealizedGains: 0
  },
  fetch_holding_detail: {
    holding: {
      commodityGuid: 'c-aapl',
      mnemonic: 'AAPL',
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
    dividendAccounts: [{ accountGuid: 'div', accountName: 'Dividends' }]
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
