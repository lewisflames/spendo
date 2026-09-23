/**
 * Fixture book for the browser dev mock (see `mock-invoke.ts`).
 *
 * Loaded only when the SvelteKit dev server runs outside the Tauri runtime, so
 * frontend/UI work can happen in a plain browser. Raw split values balance to
 * zero; the mock derives display amounts by negating credit-normal account
 * types, mirroring the backend's `utils::signed_amount_expr`.
 */

/** Local "today" (`YYYY-MM-DD`), matching the backend's `date('now','localtime')`. */
export function todayIso(): string {
  const d = new Date();
  const month = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${month}-${day}`;
}

export const REPORT_CURRENCY = 'USD';

/** Account types whose display amount negates the raw split value. Mirrors `utils::CREDIT_NORMAL`. */
export const CREDIT_NORMAL_TYPES = new Set([
  'INCOME',
  'LIABILITY',
  'EQUITY',
  'CREDIT',
  'PAYABLE',
  'TRADING'
]);

export const INVESTMENT_TYPES = new Set(['STOCK', 'MUTUAL', 'FUND']);

const TYPE_TO_CATEGORY: Record<string, string> = {
  ASSET: 'ASSET',
  BANK: 'ASSET',
  CASH: 'ASSET',
  STOCK: 'ASSET',
  MUTUAL: 'ASSET',
  FUND: 'ASSET',
  CURRENCY: 'ASSET',
  RECEIVABLE: 'ASSET',
  LIABILITY: 'LIABILITY',
  CREDIT: 'LIABILITY',
  PAYABLE: 'LIABILITY',
  INCOME: 'INCOME',
  EXPENSE: 'EXPENSE',
  EQUITY: 'EQUITY',
  TRADING: 'EQUITY'
};

export function categoryOf(accountType: string): string {
  return TYPE_TO_CATEGORY[accountType] ?? accountType;
}

export interface FixtureAccount {
  guid: string;
  parentGuid: string | null;
  name: string;
  accountType: string;
  code: string | null;
  description: string | null;
  placeholder: boolean;
  commodity: string | null;
  commodityGuid: string | null;
}

const USD = { commodity: 'USD', commodityGuid: 'c-usd' };

export const ACCOUNTS: FixtureAccount[] = [
  {
    guid: 'root',
    parentGuid: null,
    name: 'Root',
    accountType: 'ROOT',
    code: null,
    description: null,
    placeholder: true,
    commodity: null,
    commodityGuid: null
  },

  // Assets ────────────────────────────────────────────────────────────────
  {
    guid: 'assets',
    parentGuid: 'root',
    name: 'Assets',
    accountType: 'ASSET',
    code: null,
    description: null,
    placeholder: true,
    ...USD
  },
  {
    guid: 'checking',
    parentGuid: 'assets',
    name: 'Everyday Checking',
    accountType: 'BANK',
    code: '1000',
    description: 'Main account for bills and day-to-day spending',
    placeholder: false,
    ...USD
  },
  {
    guid: 'savings',
    parentGuid: 'assets',
    name: 'High-Yield Savings',
    accountType: 'BANK',
    code: '1100',
    description: null,
    placeholder: false,
    ...USD
  },
  {
    guid: 'brokerage',
    parentGuid: 'assets',
    name: 'Brokerage',
    accountType: 'ASSET',
    code: '1200',
    description: 'Long-term investments',
    placeholder: true,
    ...USD
  },
  {
    guid: 'vti',
    parentGuid: 'brokerage',
    name: 'VTI · Vanguard Total Stock Market',
    accountType: 'STOCK',
    code: null,
    description: null,
    placeholder: false,
    commodity: 'VTI',
    commodityGuid: 'c-vti'
  },
  {
    guid: 'aapl',
    parentGuid: 'brokerage',
    name: 'AAPL · Apple Inc.',
    accountType: 'STOCK',
    code: null,
    description: null,
    placeholder: false,
    commodity: 'AAPL',
    commodityGuid: 'c-aapl'
  },

  // Liabilities ───────────────────────────────────────────────────────────
  {
    guid: 'liabilities',
    parentGuid: 'root',
    name: 'Liabilities',
    accountType: 'LIABILITY',
    code: null,
    description: null,
    placeholder: true,
    ...USD
  },
  {
    guid: 'visa',
    parentGuid: 'liabilities',
    name: 'Visa Platinum',
    accountType: 'CREDIT',
    code: '2000',
    description: 'Rewards card, paid in full monthly',
    placeholder: false,
    ...USD
  },

  // Equity ────────────────────────────────────────────────────────────────
  {
    guid: 'equity',
    parentGuid: 'root',
    name: 'Equity',
    accountType: 'EQUITY',
    code: null,
    description: null,
    placeholder: true,
    ...USD
  },
  {
    guid: 'opening',
    parentGuid: 'equity',
    name: 'Opening Balances',
    accountType: 'EQUITY',
    code: '3000',
    description: null,
    placeholder: false,
    ...USD
  },

  // Income ────────────────────────────────────────────────────────────────
  {
    guid: 'income',
    parentGuid: 'root',
    name: 'Income',
    accountType: 'INCOME',
    code: null,
    description: null,
    placeholder: true,
    ...USD
  },
  {
    guid: 'salary',
    parentGuid: 'income',
    name: 'Salary',
    accountType: 'INCOME',
    code: '4000',
    description: null,
    placeholder: false,
    ...USD
  },
  {
    guid: 'interest',
    parentGuid: 'income',
    name: 'Interest',
    accountType: 'INCOME',
    code: '4100',
    description: null,
    placeholder: false,
    ...USD
  },

  // Expenses ──────────────────────────────────────────────────────────────
  {
    guid: 'expenses',
    parentGuid: 'root',
    name: 'Expenses',
    accountType: 'EXPENSE',
    code: null,
    description: null,
    placeholder: true,
    ...USD
  },
  {
    guid: 'rent',
    parentGuid: 'expenses',
    name: 'Rent',
    accountType: 'EXPENSE',
    code: '5000',
    description: 'Harborview Apartments',
    placeholder: false,
    ...USD
  },
  {
    guid: 'groceries',
    parentGuid: 'expenses',
    name: 'Groceries',
    accountType: 'EXPENSE',
    code: '5100',
    description: null,
    placeholder: false,
    ...USD
  },
  {
    guid: 'dining',
    parentGuid: 'expenses',
    name: 'Dining',
    accountType: 'EXPENSE',
    code: '5200',
    description: null,
    placeholder: false,
    ...USD
  },
  {
    guid: 'utilities',
    parentGuid: 'expenses',
    name: 'Utilities',
    accountType: 'EXPENSE',
    code: '5300',
    description: 'Power, water and internet',
    placeholder: false,
    ...USD
  },
  {
    guid: 'subscriptions',
    parentGuid: 'expenses',
    name: 'Subscriptions',
    accountType: 'EXPENSE',
    code: '5400',
    description: null,
    placeholder: false,
    ...USD
  },
  {
    guid: 'shopping',
    parentGuid: 'expenses',
    name: 'Shopping',
    accountType: 'EXPENSE',
    code: '5500',
    description: null,
    placeholder: false,
    ...USD
  },
  {
    guid: 'insurance',
    parentGuid: 'expenses',
    name: 'Insurance',
    accountType: 'EXPENSE',
    code: '5600',
    description: null,
    placeholder: false,
    ...USD
  }
];

export interface FixtureSplit {
  accountGuid: string;
  /** Raw balanced value; display amount is derived per account type. */
  raw: number;
  memo?: string | null;
  quantity?: number | null;
  /** GnuCash reconcile state: 'n' new (default), 'c' cleared, 'y' reconciled. */
  reconcileState?: string | null;
}

export interface FixtureTx {
  guid: string;
  postDate: string;
  description: string;
  num?: string | null;
  notes?: string | null;
  splits: FixtureSplit[];
}

interface TxExtra {
  num?: string;
  notes?: string;
}

function buildTransactions(): FixtureTx[] {
  const out: FixtureTx[] = [];

  const add = (
    guid: string,
    postDate: string,
    description: string,
    splits: FixtureSplit[],
    extra: TxExtra = {}
  ) => {
    out.push({
      guid,
      postDate,
      description,
      num: extra.num ?? null,
      notes: extra.notes ?? null,
      splits
    });
  };

  // Opening balances (raw signs balance to zero).
  add('tx-opening', '2026-01-05', 'Opening balances', [
    { accountGuid: 'checking', raw: 12_500 },
    { accountGuid: 'savings', raw: 18_000 },
    { accountGuid: 'vti', raw: 6_000, quantity: 20 },
    { accountGuid: 'visa', raw: -1_240 },
    { accountGuid: 'opening', raw: -35_260 }
  ]);

  const months = ['2026-04', '2026-05', '2026-06', '2026-07', '2026-08', '2026-09'];
  const groceryAmounts = [142.18, 96.42, 158.75, 121.3];
  const diningAmounts = [48.2, 72.5, 36.9];

  months.forEach((month, i) => {
    add(
      `tx-${month}-salary-1`,
      `${month}-01`,
      'Acme Corp payroll',
      [
        { accountGuid: 'checking', raw: 3_200, memo: 'Direct deposit' },
        { accountGuid: 'salary', raw: -3_200 }
      ],
      { num: `PR-${400 + i}` }
    );

    add(
      `tx-${month}-salary-2`,
      `${month}-15`,
      'Acme Corp payroll',
      [
        { accountGuid: 'checking', raw: 3_200, memo: 'Direct deposit' },
        { accountGuid: 'salary', raw: -3_200 }
      ],
      { num: `PR-${400 + i}` }
    );

    add(`tx-${month}-rent`, `${month}-01`, 'Harborview Apartments rent', [
      { accountGuid: 'checking', raw: -1_450 },
      { accountGuid: 'rent', raw: 1_450 }
    ]);

    add(`tx-${month}-netflix`, `${month}-05`, 'Netflix', [
      { accountGuid: 'visa', raw: -15.49 },
      { accountGuid: 'subscriptions', raw: 15.49 }
    ]);

    add(`tx-${month}-spotify`, `${month}-10`, 'Spotify Premium', [
      { accountGuid: 'visa', raw: -10.99 },
      { accountGuid: 'subscriptions', raw: 10.99 }
    ]);

    add(`tx-${month}-power`, `${month}-08`, 'City Power & Light', [
      { accountGuid: 'checking', raw: -89.99 },
      { accountGuid: 'utilities', raw: 89.99 }
    ]);

    add(`tx-${month}-water`, `${month}-18`, 'Municipal water', [
      { accountGuid: 'checking', raw: -64.3 },
      { accountGuid: 'utilities', raw: 64.3 }
    ]);

    add(`tx-${month}-transfer`, `${month}-16`, 'Transfer to savings', [
      { accountGuid: 'checking', raw: -500 },
      { accountGuid: 'savings', raw: 500 }
    ]);

    add(`tx-${month}-card-payment`, `${month}-20`, 'Visa payment', [
      { accountGuid: 'checking', raw: -620 },
      { accountGuid: 'visa', raw: 620 }
    ]);

    add(`tx-${month}-shopping`, `${month}-11`, 'Amazon order', [
      { accountGuid: 'visa', raw: -(40 + i * 7) },
      { accountGuid: 'shopping', raw: 40 + i * 7 }
    ]);

    add(`tx-${month}-interest`, `${month}-28`, 'Interest · High-Yield Savings', [
      { accountGuid: 'savings', raw: 19.42 + i },
      { accountGuid: 'interest', raw: -(19.42 + i) }
    ]);

    [3, 12, 21, 27].forEach((day, n) => {
      const amount = groceryAmounts[(i + n) % groceryAmounts.length];
      add(
        `tx-${month}-groceries-${n}`,
        `${month}-${String(day).padStart(2, '0')}`,
        'Whole Foods Market',
        [
          { accountGuid: 'checking', raw: -amount },
          { accountGuid: 'groceries', raw: amount }
        ]
      );
    });

    [6, 14, 22].forEach((day, n) => {
      const amount = diningAmounts[(i + n) % diningAmounts.length];
      const places = ['Blue Bottle Coffee', 'Sushi Nakamura', 'Taqueria Vista'];
      add(`tx-${month}-dining-${n}`, `${month}-${String(day).padStart(2, '0')}`, places[n], [
        { accountGuid: 'visa', raw: -amount },
        { accountGuid: 'dining', raw: amount }
      ]);
    });
  });

  add(
    'tx-buy-vti',
    '2026-05-06',
    'Buy 10 VTI',
    [
      { accountGuid: 'checking', raw: -3_000 },
      { accountGuid: 'vti', raw: 3_000, quantity: 10 }
    ],
    { notes: 'Dollar-cost averaging' }
  );

  add('tx-buy-aapl', '2026-07-15', 'Buy 8 AAPL', [
    { accountGuid: 'checking', raw: -2_000 },
    { accountGuid: 'aapl', raw: 2_000, quantity: 8 }
  ]);

  add(
    'tx-insurance',
    '2026-02-12',
    'Auto insurance premium',
    [
      { accountGuid: 'checking', raw: -780 },
      { accountGuid: 'insurance', raw: 780 }
    ],
    { notes: 'Paid annually' }
  );

  return out.filter((tx) => tx.postDate <= todayIso());
}

export const TRANSACTIONS: FixtureTx[] = buildTransactions();

export type FixtureFrequency = 'daily' | 'weekly' | 'biweekly' | 'monthly' | 'quarterly' | 'yearly';

export interface FixtureScheduleDef {
  guid: string;
  name: string;
  frequency: FixtureFrequency;
  intervalMult: number;
  startDate: string;
  endDate: string | null;
  maxOccurrences: number | null;
  description: string | null;
  num: string | null;
  notes: string | null;
  active: boolean;
  splits: { accountGuid: string; amount: number; memo?: string | null }[];
}

export const SCHEDULES: FixtureScheduleDef[] = [
  {
    guid: 'sched-rent',
    name: 'Rent',
    frequency: 'monthly',
    intervalMult: 1,
    startDate: '2026-04-01',
    endDate: null,
    maxOccurrences: null,
    description: 'Harborview Apartments',
    num: null,
    notes: null,
    active: true,
    splits: [
      { accountGuid: 'checking', amount: -1_450 },
      { accountGuid: 'rent', amount: 1_450 }
    ]
  },
  {
    guid: 'sched-internet',
    name: 'Internet',
    frequency: 'monthly',
    intervalMult: 1,
    startDate: '2026-04-08',
    endDate: null,
    maxOccurrences: null,
    description: 'Fiber 1 Gb',
    num: null,
    notes: null,
    active: true,
    splits: [
      { accountGuid: 'checking', amount: -89.99 },
      { accountGuid: 'utilities', amount: 89.99 }
    ]
  },
  {
    guid: 'sched-netflix',
    name: 'Netflix',
    frequency: 'monthly',
    intervalMult: 1,
    startDate: '2026-04-05',
    endDate: null,
    maxOccurrences: null,
    description: 'Standard plan',
    num: null,
    notes: null,
    active: true,
    splits: [
      { accountGuid: 'visa', amount: -15.49 },
      { accountGuid: 'subscriptions', amount: 15.49 }
    ]
  },
  {
    guid: 'sched-insurance',
    name: 'Auto insurance',
    frequency: 'yearly',
    intervalMult: 1,
    startDate: '2026-02-12',
    endDate: null,
    maxOccurrences: null,
    description: null,
    num: null,
    notes: 'Paid annually',
    active: true,
    splits: [
      { accountGuid: 'checking', amount: -780 },
      { accountGuid: 'insurance', amount: 780 }
    ]
  }
];

export interface FixtureBudgetPlan {
  accountGuid: string;
  year: number;
  amount: number;
}

export const BUDGET_PLANS: FixtureBudgetPlan[] = [
  { accountGuid: 'rent', year: 2026, amount: 17_400 },
  { accountGuid: 'groceries', year: 2026, amount: 6_000 },
  { accountGuid: 'dining', year: 2026, amount: 3_000 },
  { accountGuid: 'utilities', year: 2026, amount: 2_400 },
  { accountGuid: 'subscriptions', year: 2026, amount: 320 },
  { accountGuid: 'shopping', year: 2026, amount: 2_400 }
];

export interface FixtureCommodity {
  guid: string;
  mnemonic: string;
  fullName: string | null;
  namespace: string;
}

export const COMMODITIES: FixtureCommodity[] = [
  { guid: 'c-usd', mnemonic: 'USD', fullName: 'US Dollar', namespace: 'CURRENCY' },
  { guid: 'c-eur', mnemonic: 'EUR', fullName: 'Euro', namespace: 'CURRENCY' },
  { guid: 'c-gbp', mnemonic: 'GBP', fullName: 'Pound Sterling', namespace: 'CURRENCY' },
  {
    guid: 'c-vti',
    mnemonic: 'VTI',
    fullName: 'Vanguard Total Stock Market ETF',
    namespace: 'NASDAQ'
  },
  { guid: 'c-aapl', mnemonic: 'AAPL', fullName: 'Apple Inc.', namespace: 'NASDAQ' }
];

export const MARKET_PRICES: Record<
  string,
  { price: number; prevPrice: number; priceDate: string }
> = {
  VTI: { price: 324.18, prevPrice: 321.4, priceDate: '2026-09-19' },
  AAPL: { price: 245.63, prevPrice: 247.1, priceDate: '2026-09-19' }
};

/** 1×1 transparent PNG, good enough for attachment previews in the mock. */
export const SAMPLE_IMAGE_DATA_URL =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

export const SAMPLE_TEXT_DATA_URL = 'data:text/plain;base64,U3BlbmRvIG1vY2sgYXR0YWNobWVudC4=';
