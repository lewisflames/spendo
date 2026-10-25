import { expect, mockBackend, test, type BackendMocks } from './fixtures';

const BASE: BackendMocks = {
  fetch_active_book: { path: '/tmp/test.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: 'EUR',
      totalAccounts: 6,
      totalTransactions: 40,
      earliestDate: '2025-01-01',
      latestDate: '2026-06-30',
      netWorth: 10000
    }
  },
  fetch_nav_badges: { dueSchedules: 0, unclearedTransactions: 0 },
  fetch_tags: [{ guid: 'g-tax', name: 'Deductible', transactionCount: 2, lastUsed: '2025-05-01' }]
};

const line = (accountGuid: string, accountName: string, accountCode: string, amount: number) => ({
  accountGuid,
  accountName,
  accountCode,
  amount
});
const section = (...lines: ReturnType<typeof line>[]) => ({
  lines,
  total: lines.reduce((sum, l) => sum + l.amount, 0)
});
const empty = { lines: [], total: 0 };

const SUMMARY = {
  year: 2025,
  years: [2024, 2025],
  reportCurrency: 'EUR',
  isPartial: false,
  settlementYear: 2026,
  costBasisMethod: 'average',
  employmentIncome: section(line('payroll', 'payroll', '4400', 32525.05)),
  employmentWithholding: section(line('irpf', 'payroll-irpf', '5640', 5452.6)),
  socialContributions: section(line('ss', 'payroll-ss', '5650', 2034.06)),
  savingsIncome: section(line('deposits', 'deposits', '4100', 570.62)),
  savingsWithholding: section(line('dep-irpf', 'deposits-irpf', '5610', 108.43)),
  settlement: section(line('settle', 'irpf-settlement', '5670', 77.43)),
  exempt: section(line('gifts', 'gifts', '4200', 240)),
  realized: { gains: 120, losses: -20, net: 100, disposals: 2 },
  deductible: { tagGuid: 'g-tax', tagName: 'Deductible', total: 150, transactions: 2 },
  untagged: [line('other', 'Side gigs', '4900', 300)],
  grossIncome: 33195.67,
  incomeTax: 5638.46,
  effectiveRate: 0.1698,
  missingFx: []
};

test('sums one year with the next year’s settlement and links each account', async ({ page }) => {
  await mockBackend(page, { ...BASE, fetch_tax_summary: SUMMARY });
  await page.goto('/reports/tax?year=2025');

  await expect(page.getByRole('heading', { name: 'Tax summary' })).toBeVisible();
  await expect(page.getByText('Annual return, settled in 2026')).toBeVisible();
  await expect(page.getByText('Income tax for 2025')).toBeVisible();
  // Account rows open that account's ledger for the year the amount is from.
  await expect(page.getByRole('link', { name: /payroll-irpf/ })).toHaveAttribute(
    'href',
    '/accounts/irpf?from=2025-01-01&to=2025-12-31'
  );
  await expect(page.getByRole('link', { name: /irpf-settlement/ })).toHaveAttribute(
    'href',
    '/accounts/settle?from=2026-01-01&to=2026-12-31'
  );
  // Untagged income is called out, average cost gets a FIFO note.
  await expect(page.getByText(/Side gigs/)).toBeVisible();
  await expect(page.getByText(/Lots use average cost/)).toBeVisible();
  await expect(page.getByRole('link', { name: 'See them in Transactions' })).toHaveAttribute(
    'href',
    '/transactions?tag=g-tax&from=2025-01-01&to=2025-12-31'
  );
});

test('offers to set up roles when nothing is tagged yet', async ({ page }) => {
  await mockBackend(page, {
    ...BASE,
    fetch_tax_summary: {
      ...SUMMARY,
      employmentIncome: empty,
      employmentWithholding: empty,
      socialContributions: empty,
      savingsIncome: empty,
      savingsWithholding: empty,
      settlement: empty,
      exempt: empty,
      realized: { gains: 0, losses: 0, net: 0, disposals: 0 },
      deductible: null
    },
    fetch_tax_roles: [],
    fetch_accounts: {
      trees: [
        {
          guid: 'income',
          name: 'Income',
          accountType: 'INCOME',
          code: null,
          description: null,
          placeholder: true,
          commodity: 'EUR',
          commodityGuid: 'eur',
          balance: 0,
          txCount: 0,
          children: [
            {
              guid: 'payroll',
              name: 'Salary',
              accountType: 'INCOME',
              code: '4400',
              description: null,
              placeholder: false,
              commodity: 'EUR',
              commodityGuid: 'eur',
              balance: 0,
              txCount: 0,
              children: []
            }
          ]
        }
      ],
      byType: {},
      totalAccounts: 2,
      reportCurrency: 'EUR',
      retainedEarnings: 0,
      unrealizedGains: 0
    },
    save_tax_roles: [{ accountGuid: 'payroll', role: 'employment_income' }]
  });
  await page.goto('/reports/tax?year=2025');

  await page.getByRole('button', { name: 'Set up tax roles' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Tax role for Salary').click();
  await page.getByRole('option', { name: /Employment income/ }).click();
  await dialog.getByRole('button', { name: 'Save roles' }).click();
  await expect(dialog).toHaveCount(0);
});
