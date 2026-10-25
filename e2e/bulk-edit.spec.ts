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

const row = (
  guid: string,
  description: string,
  postDate: string,
  tags: { guid: string; name: string }[] = [],
  payee: { guid: string; name: string } | null = null
) => ({
  guid,
  postDate,
  description,
  num: null,
  notes: null,
  totalAmount: -10,
  tags,
  payee,
  splits: [
    { guid: `${guid}-a`, accountGuid: 'food', accountName: 'Groceries', amount: 10 },
    { guid: `${guid}-b`, accountGuid: 'bank', accountName: 'Bank', amount: -10 }
  ]
});

/** Three rows on the page out of five matching. */
const LEDGER: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 3,
      totalTransactions: 5,
      earliestDate: '2024-01-01',
      latestDate: '2024-01-03',
      netWorth: 0
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_transaction_heatmap: [],
  fetch_transactions: {
    transactions: [
      row('t1', 'Corner shop', '2024-01-03', [{ guid: 'g-trip', name: 'Summer trip' }], {
        guid: 'p-renfe',
        name: 'Renfe'
      }),
      row('t2', 'Market', '2024-01-02'),
      row('t3', 'Bakery', '2024-01-01')
    ],
    total: 5,
    page: 1,
    pageSize: 3,
    totalPages: 2
  },
  fetch_accounts: {
    trees: [
      account('bank', 'Bank', 'BANK'),
      account('food', 'Groceries', 'EXPENSE'),
      account('dining', 'Dining', 'EXPENSE')
    ],
    byType: {},
    totalAccounts: 3,
    reportCurrency: 'EUR',
    retainedEarnings: 0,
    unrealizedGains: 0
  },
  fetch_tags: [
    { guid: 'g-trip', name: 'Summer trip', transactionCount: 4, lastUsed: '2024-01-03' },
    { guid: 'g-tax', name: 'Deductible', transactionCount: 1, lastUsed: '2023-12-01' }
  ],
  fetch_payees: [
    { guid: 'p-renfe', name: 'Renfe', transactionCount: 18, lastUsed: '2024-01-03' },
    { guid: 'p-gadis', name: 'Gadis', transactionCount: 15, lastUsed: '2023-12-20' }
  ],
  bulk_edit_transactions: { changed: 3, skippedLocked: 1 }
};

/** Record every IPC call so specs can assert on the payload. */
async function openLedger(page: Page, path = '/transactions') {
  await mockBackend(page, LEDGER);
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
  await page.goto(path);
  await expect(page.getByRole('link', { name: 'Corner shop' })).toBeVisible();
}

async function bulkCall(page: Page) {
  return page.evaluate(
    () =>
      (window as unknown as { __calls: { cmd: string; args: unknown }[] }).__calls.find(
        (c) => c.cmd === 'bulk_edit_transactions'
      )?.args
  );
}

test('shift-click selects a range and rename sends those rows', async ({ page }) => {
  await openLedger(page);
  const toolbar = page.getByRole('toolbar', { name: 'Bulk actions' });
  await expect(toolbar).toHaveCount(0);

  // The cell also holds the row's tag.
  const firstRow = page.getByRole('cell', { name: 'Corner shop Summer trip', exact: true });
  const before = await firstRow.boundingBox();
  await page.getByRole('checkbox', { name: 'Select Corner shop' }).click();
  await expect(toolbar).toContainText('1 selected');
  // The toolbar appears without pushing the rows down.
  expect((await firstRow.boundingBox())?.y).toBe(before?.y);
  await page.getByRole('cell', { name: 'Bakery', exact: true }).click({ modifiers: ['Shift'] });
  await expect(toolbar).toContainText('3 selected');
  // Shift-click selected instead of navigating.
  await expect(page).toHaveURL(/\/transactions$/);

  await toolbar.getByRole('button', { name: 'Rename' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Description').fill('Groceries');
  await dialog.getByRole('button', { name: 'Rename' }).click();

  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('Renamed 3 transactions · 1 cleared or reconciled')).toBeVisible();
  await expect(toolbar).toHaveCount(0);
  expect(await bulkCall(page)).toEqual({
    input: {
      target: { kind: 'guids', guids: ['t1', 't2', 't3'] },
      action: { kind: 'setDescription', description: 'Groceries' }
    }
  });
});

test('select all matching moves splits by filter', async ({ page }) => {
  await openLedger(page);
  await page.getByRole('checkbox', { name: 'Select the transactions on this page' }).click();
  const toolbar = page.getByRole('toolbar', { name: 'Bulk actions' });
  await toolbar.getByRole('button', { name: 'Select all 5 matching transactions' }).click();
  await expect(toolbar).toContainText('All 5 transactions selected');

  await toolbar.getByRole('button', { name: 'Move' }).click();
  const dialog = page.getByRole('dialog');
  // The busiest category is the default source.
  await expect(dialog.getByRole('combobox', { name: 'From' })).toContainText('Groceries');
  await dialog.getByRole('combobox', { name: 'To' }).click();
  await page.getByRole('option', { name: 'Dining' }).click();
  await dialog.getByRole('button', { name: 'Move' }).click();

  await expect(dialog).toHaveCount(0);
  expect(await bulkCall(page)).toEqual({
    input: {
      target: { kind: 'matching', filters: null, expectedCount: 5 },
      action: { kind: 'moveSplits', fromAccountGuid: 'food', toAccountGuid: 'dining' }
    }
  });
});

test('delete asks before removing the selection', async ({ page }) => {
  await openLedger(page);
  await page.getByRole('checkbox', { name: 'Select Market' }).click();
  await page
    .getByRole('toolbar', { name: 'Bulk actions' })
    .getByRole('button', { name: 'Delete' })
    .click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('A backup is taken first');
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  expect(await bulkCall(page)).toBeUndefined();

  await page
    .getByRole('toolbar', { name: 'Bulk actions' })
    .getByRole('button', { name: 'Delete' })
    .click();
  await dialog.getByRole('button', { name: 'Yes, delete 1 transaction' }).click();
  expect(await bulkCall(page)).toEqual({
    input: { target: { kind: 'guids', guids: ['t2'] }, action: { kind: 'delete' } }
  });
});

test('tags show on rows, filter the list and bulk tagging sends the name', async ({ page }) => {
  await openLedger(page);
  // The row's tag links to the list filtered by it.
  await expect(page.getByRole('link', { name: 'Summer trip' })).toHaveAttribute(
    'href',
    '/transactions?tag=g-trip'
  );

  await page.getByRole('checkbox', { name: 'Select Market' }).click();
  await page.getByRole('checkbox', { name: 'Select Bakery' }).click();
  await page
    .getByRole('toolbar', { name: 'Bulk actions' })
    .getByRole('button', { name: 'Tag' })
    .click();
  const dialog = page.getByRole('dialog');
  // Neither selected row has a tag, so there is nothing to remove.
  await expect(dialog.getByRole('tab', { name: 'Remove a tag' })).toBeDisabled();
  await dialog.getByLabel('Tag').fill('deduct');
  await page.getByRole('option', { name: /Deductible/ }).click();
  await dialog.getByRole('button', { name: 'Add tag' }).click();

  await expect(dialog).toHaveCount(0);
  expect(await bulkCall(page)).toEqual({
    input: {
      target: { kind: 'guids', guids: ['t2', 't3'] },
      action: { kind: 'addTag', name: 'Deductible' }
    }
  });
});

test('the tag filter narrows the list request', async ({ page }) => {
  await openLedger(page);
  await page.getByLabel('Tag').click();
  await page.getByRole('option', { name: 'Summer trip' }).click();
  await expect(page).toHaveURL(/tag=g-trip/);
  const lastList = await page.evaluate(
    () =>
      (window as unknown as { __calls: { cmd: string; args: { filters?: unknown } }[] }).__calls
        .filter((c) => c.cmd === 'fetch_transactions')
        .at(-1)?.args.filters
  );
  expect(lastList).toEqual({ tagGuid: 'g-trip' });
});

test('payees link to a filtered list and bulk set payee sends the name', async ({ page }) => {
  await openLedger(page);
  await expect(page.getByRole('link', { name: 'Renfe' })).toHaveAttribute(
    'href',
    '/transactions?payee=p-renfe'
  );

  await page.getByRole('checkbox', { name: 'Select Market' }).click();
  await page.getByRole('checkbox', { name: 'Select Bakery' }).click();
  await page
    .getByRole('toolbar', { name: 'Bulk actions' })
    .getByRole('button', { name: 'Payee' })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Payee').fill('ren');
  await page.getByRole('option', { name: /Renfe/ }).click();
  await dialog.getByRole('button', { name: 'Set payee' }).click();

  await expect(dialog).toHaveCount(0);
  expect(await bulkCall(page)).toEqual({
    input: {
      target: { kind: 'guids', guids: ['t2', 't3'] },
      action: { kind: 'setPayee', name: 'Renfe' }
    }
  });
});

test('the payee filter comes from the URL and clears from its chip', async ({ page }) => {
  await openLedger(page, '/transactions?payee=p-renfe');
  const lastFilters = () =>
    page.evaluate(
      () =>
        (window as unknown as { __calls: { cmd: string; args: { filters?: unknown } }[] }).__calls
          .filter((c) => c.cmd === 'fetch_transactions')
          .at(-1)?.args.filters
    );
  await expect.poll(lastFilters).toEqual({ payeeGuid: 'p-renfe' });

  await page.getByRole('button', { name: 'Stop filtering by Renfe' }).click();
  await expect(page).not.toHaveURL(/payee=/);
  await expect.poll(lastFilters).toEqual({});
});
