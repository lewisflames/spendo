import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const accounts = (checking: number) => ({
  trees: [
    {
      guid: 'assets',
      name: 'Assets',
      accountType: 'ASSET',
      code: null,
      description: null,
      placeholder: true,
      commodity: 'EUR',
      commodityGuid: 'c-eur',
      balance: checking,
      txCount: 1,
      children: [
        {
          guid: 'checking',
          name: 'Checking',
          accountType: 'BANK',
          code: null,
          description: null,
          placeholder: false,
          commodity: 'EUR',
          commodityGuid: 'c-eur',
          balance: checking,
          txCount: 1,
          children: []
        }
      ]
    }
  ],
  byType: { ASSET: { count: 2, totalBalance: checking } },
  totalAccounts: 2,
  reportCurrency: 'EUR',
  retainedEarnings: checking,
  unrealizedGains: 0,
  balanceCheck: 0,
  missingFx: [],
  missingPrices: []
});

const BASE: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 2,
      totalTransactions: 2,
      earliestDate: '2025-01-10',
      latestDate: '2026-06-01',
      netWorth: 300
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 }
};

test('shows one amount column for today by default', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_balance_sheet: {
      asOf: '2026-06-30',
      current: accounts(300),
      compareTo: null,
      compare: null
    }
  });
  await page.goto('/reports/balance-sheet');

  await expect(page.getByRole('link', { name: 'Checking' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: 'Change' })).toHaveCount(0);
  await expect(page.getByText('Balanced. Everything adds up.')).toBeVisible();
});

test('asks for the as-of and comparison dates from the URL', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_balance_sheet: {
      asOf: '2026-06-30',
      current: accounts(300),
      compareTo: '2025-12-31',
      compare: accounts(100)
    }
  });
  await page.addInitScript(() => {
    type Invoke = (cmd: string, args: unknown) => Promise<unknown>;
    const internals = (window as unknown as Record<string, { invoke: Invoke }>).__TAURI_INTERNALS__;
    const invoke = internals.invoke;
    const calls: { cmd: string; args: unknown }[] = [];
    Object.assign(window, { __calls: calls });
    internals.invoke = (cmd: string, args: unknown) => {
      calls.push({ cmd, args });
      return invoke(cmd, args);
    };
  });
  await page.goto('/reports/balance-sheet?asOf=2026-06-30&compare=year');

  await expect(page.getByRole('columnheader', { name: 'Change' }).first()).toBeVisible();
  const args = await page.evaluate(
    () =>
      (window as unknown as { __calls: { cmd: string; args: unknown }[] }).__calls.findLast(
        (c) => c.cmd === 'fetch_balance_sheet'
      )?.args
  );
  expect(args).toEqual({ input: { asOf: '2026-06-30', compareTo: '2025-12-31' } });
  // Checking went from 100 to 300.
  await expect(page.getByText('+€200.00').first()).toBeVisible();
});
