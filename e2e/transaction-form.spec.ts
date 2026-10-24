import type { Page } from '@playwright/test';
import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const account = (guid: string, name: string, accountType: string) => ({
  guid,
  name,
  accountType,
  code: null,
  description: null,
  placeholder: false,
  commodity: 'EUR',
  commodityGuid: 'eur',
  balance: 0,
  txCount: 0,
  children: []
});

/** An open book with two accounts and nothing due. */
const OPEN_BOOK: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 2,
      totalTransactions: 0,
      earliestDate: null,
      latestDate: null,
      netWorth: 0
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_accounts: {
    trees: [account('bank', 'Bank', 'BANK'), account('food', 'Groceries', 'EXPENSE')],
    byType: {},
    totalAccounts: 2,
    reportCurrency: 'EUR',
    retainedEarnings: 0,
    unrealizedGains: 0
  }
};

const RECIPES = Array.from({ length: 8 }, (_, i) => ({
  description: `Groceries shop ${i}`,
  splits: [
    { accountGuid: 'food', amount: '10', quantity: '', showQuantity: false, memo: '' },
    { accountGuid: 'bank', amount: '-10', quantity: '', showQuantity: false, memo: '' }
  ]
}));

async function openNewTransaction(page: Page) {
  await mockBackend(page, OPEN_BOOK);
  await page.addInitScript((recipes) => {
    localStorage.setItem('tx-recent-recipes', JSON.stringify(recipes));
  }, RECIPES);
  await page.goto('/transactions/new');
  await expect(page.getByRole('heading', { name: 'New transaction' })).toBeVisible();
  return page.getByRole('combobox', { name: 'Description' });
}

test('arrow keys move through description suggestions', async ({ page }) => {
  const input = await openNewTransaction(page);
  await input.pressSequentially('gro');
  const options = page.getByRole('option');
  await expect(options).toHaveCount(8);

  await input.press('ArrowDown');
  await expect(options.nth(0)).toHaveAttribute('aria-selected', 'true');
  for (let i = 0; i < 6; i++) await input.press('ArrowDown');
  await expect(options.nth(6)).toHaveAttribute('aria-selected', 'true');
  await expect(options.nth(6)).toBeInViewport();

  await input.press('Enter');
  await expect(input).toHaveValue('Groceries shop 6');
  await expect(options).toHaveCount(0);
});

test('typing a description alone does not flag the untouched splits', async ({ page }) => {
  const input = await openNewTransaction(page);
  await input.pressSequentially('Coffee');
  await expect(page.getByText('Add an amount to every split.')).toBeHidden();
  await expect(page.getByText(/choose an account/)).toHaveCount(0);
});
