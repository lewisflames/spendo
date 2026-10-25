import type { BackendMocks } from '../fixtures';

/**
 * A made-up household book for the README and landing-page screenshots. Every
 * figure derives from the same monthly series, so the overview, reports,
 * budgets and account balances agree with each other. Payees are invented.
 */

/** The screenshots freeze the browser clock here. */
export const TODAY = '2026-10-20';
const CURRENCY = 'EUR';

/** Deterministic noise in [0, 1): mulberry32 seeded from the inputs. */
function noise(...keys: (string | number)[]): number {
  let h = 2166136261;
  for (const ch of keys.join('|')) h = Math.imul(h ^ ch.charCodeAt(0), 16777619);
  let t = h + 0x6d2b79f5;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

const round2 = (n: number) => Math.round(n * 100) / 100;
const sum = (xs: number[]) => round2(xs.reduce((a, b) => a + b, 0));
const pad = (n: number) => String(n).padStart(2, '0');

/** `YYYY-MM` for every month from `from` through `to`, inclusive. */
function monthsBetween(from: string, to: string): string[] {
  const out: string[] = [];
  let [y, m] = from.split('-').map(Number);
  const [ty, tm] = to.split('-').map(Number);
  while (y < ty || (y === ty && m <= tm)) {
    out.push(`${y}-${pad(m)}`);
    m += 1;
    if (m > 12) [y, m] = [y + 1, 1];
  }
  return out;
}

const monthEnd = (month: string) => {
  const [y, m] = month.split('-').map(Number);
  return `${month}-${pad(new Date(y, m, 0).getDate())}`;
};

const CURRENT_MONTH = TODAY.slice(0, 7);
/** Share of the current month that has happened (October 20 of 31 days). */
const MONTH_ELAPSED = Number(TODAY.slice(8)) / 31;

// ── Categories ──

type Leaf = { guid: string; name: string; monthly: (month: string) => number };
type Category = { guid: string; name: string; leaves: Leaf[] };

const month0 = (month: string) => Number(month.slice(5));
const vary = (base: number, spread: number, ...keys: (string | number)[]) =>
  round2(base + (noise(...keys) - 0.5) * 2 * spread);

const EXPENSES: Category[] = [
  {
    guid: 'exp-housing',
    name: 'Housing',
    leaves: [
      { guid: 'exp-rent', name: 'Rent', monthly: () => 1350 },
      {
        guid: 'exp-utilities',
        name: 'Utilities',
        // Heating season costs more.
        monthly: (m) => vary([11, 12, 1, 2, 3].includes(month0(m)) ? 168 : 104, 18, 'util', m)
      },
      { guid: 'exp-internet', name: 'Internet', monthly: () => 39.9 }
    ]
  },
  {
    guid: 'exp-groceries',
    name: 'Groceries',
    leaves: [{ guid: 'exp-groceries', name: 'Groceries', monthly: (m) => vary(545, 70, 'groc', m) }]
  },
  {
    guid: 'exp-dining',
    name: 'Dining out',
    leaves: [{ guid: 'exp-dining', name: 'Dining out', monthly: (m) => vary(190, 75, 'dine', m) }]
  },
  {
    guid: 'exp-transport',
    name: 'Transport',
    leaves: [
      { guid: 'exp-fuel', name: 'Fuel', monthly: (m) => vary(118, 32, 'fuel', m) },
      { guid: 'exp-transit', name: 'Public transport', monthly: () => 49 }
    ]
  },
  {
    guid: 'exp-travel',
    name: 'Travel',
    leaves: [
      {
        guid: 'exp-travel',
        name: 'Travel',
        monthly: (m) => ({ 4: 620, 8: 1850, 12: 940 })[month0(m)] ?? 0
      }
    ]
  },
  {
    guid: 'exp-shopping',
    name: 'Shopping',
    leaves: [
      {
        guid: 'exp-shopping',
        name: 'Shopping',
        monthly: (m) => vary(month0(m) === 11 ? 460 : 170, 90, 'shop', m)
      }
    ]
  },
  {
    guid: 'exp-insurance',
    name: 'Insurance',
    leaves: [{ guid: 'exp-insurance', name: 'Insurance', monthly: () => 68.4 }]
  },
  {
    guid: 'exp-health',
    name: 'Health',
    leaves: [
      {
        guid: 'exp-health',
        name: 'Health',
        monthly: (m) => (noise('health', m) > 0.55 ? vary(95, 70, 'hamt', m) : 0)
      }
    ]
  },
  {
    guid: 'exp-entertainment',
    name: 'Entertainment',
    leaves: [
      { guid: 'exp-subscriptions', name: 'Subscriptions', monthly: () => 32.97 },
      {
        guid: 'exp-events',
        name: 'Events',
        monthly: (m) => (noise('events', m) > 0.5 ? vary(64, 30, 'evt', m) : 0)
      }
    ]
  },
  {
    guid: 'exp-gifts',
    name: 'Gifts',
    leaves: [
      {
        guid: 'exp-gifts',
        name: 'Gifts',
        monthly: (m) => (month0(m) === 12 ? 430 : noise('gift', m) > 0.7 ? 45 : 0)
      }
    ]
  }
];

const LEAVES = EXPENSES.flatMap((c) => c.leaves);

/** Spending of one leaf in a month; the current month only counts what has happened. */
function leafSpend(leaf: Leaf, month: string): number {
  if (month > CURRENT_MONTH) return 0;
  const full = leaf.monthly(month);
  // Rent and the fixed bills go out on the first; the rest accrues.
  const fixed = ['exp-rent', 'exp-internet', 'exp-insurance', 'exp-transit'].includes(leaf.guid);
  return month === CURRENT_MONTH && !fixed ? round2(full * MONTH_ELAPSED) : full;
}

const categorySpend = (c: Category, month: string) => sum(c.leaves.map((l) => leafSpend(l, month)));
const totalSpend = (month: string) => sum(EXPENSES.map((c) => categorySpend(c, month)));

const SALARY = 4850;
const salary = (month: string) => (month0(month) === 12 ? SALARY * 2 : SALARY);
const interest = (month: string) => vary(34, 9, 'int', month);
const dividends = (month: string) => ([3, 6, 9, 12].includes(month0(month)) ? 186.4 : 0);
const income = (month: string) =>
  month > CURRENT_MONTH ? 0 : sum([salary(month), interest(month), dividends(month)]);

// ── Overview ──

const HISTORY = monthsBetween('2023-01', CURRENT_MONTH);

/** Final balances, so the account tree, KPIs and the trend's last point match. */
const CASH = { checking: 4212.37, savings: 18500, wallet: 86.4 };
const CARD_BALANCE = 642.18;
const CAR_LOAN = 7850;

// ── Portfolio ──

const HOLDINGS = [
  {
    guid: 'cmdty-vwrl',
    mnemonic: 'VWRL',
    fullName: 'Vanguard FTSE All-World UCITS ETF (Dist)',
    shares: 310,
    price: 128.94,
    prevPrice: 128.11,
    cost: 31680,
    realized: 0,
    dividends: 1318.6,
    xirr: 9.4
  },
  {
    guid: 'cmdty-emim',
    mnemonic: 'EMIM',
    fullName: 'iShares Core MSCI EM IMI UCITS ETF',
    shares: 260,
    price: 35.42,
    prevPrice: 35.61,
    cost: 8140,
    realized: 0,
    dividends: 0,
    xirr: 6.1
  },
  {
    guid: 'cmdty-aggh',
    mnemonic: 'AGGH',
    fullName: 'iShares Core Global Aggregate Bond UCITS ETF',
    shares: 1450,
    price: 5.12,
    prevPrice: 5.11,
    cost: 7610,
    realized: 0,
    dividends: 0,
    xirr: 1.2
  },
  {
    guid: 'cmdty-asml',
    mnemonic: 'ASML',
    fullName: 'ASML Holding N.V.',
    shares: 6,
    price: 712.4,
    prevPrice: 704.9,
    cost: 3880,
    realized: 412.5,
    dividends: 38.2,
    xirr: 11.8
  }
];

const portfolioValue = sum(HOLDINGS.map((h) => h.shares * h.price));
const portfolioCost = sum(HOLDINGS.map((h) => h.cost));

function portfolioHoldings() {
  return HOLDINGS.map((h) => {
    const marketValue = round2(h.shares * h.price);
    const unrealized = round2(marketValue - h.cost);
    return {
      commodityGuid: h.guid,
      mnemonic: h.mnemonic,
      fullName: h.fullName,
      namespace: 'XAMS',
      shares: h.shares,
      costBasis: h.cost,
      openCostBasis: h.cost,
      marketValue,
      avgCostPerShare: round2(h.cost / h.shares),
      gainLoss: unrealized,
      unrealizedGain: unrealized,
      realizedGain: h.realized,
      dividends: h.dividends,
      totalReturn: round2(unrealized + h.realized + h.dividends),
      basisWarning: null,
      xirr: h.xirr,
      allocationPct: round2((marketValue / portfolioValue) * 100),
      price: h.price,
      prevPrice: h.prevPrice,
      priceDate: TODAY,
      accounts: [
        {
          accountGuid: `acct-${h.mnemonic.toLowerCase()}`,
          accountName: h.mnemonic,
          accountDescription: h.fullName,
          accountCode: null,
          shares: h.shares,
          costBasis: h.cost
        }
      ]
    };
  });
}

/** Steady monthly buying with a market that wobbles but trends up, ending at today's values. */
function portfolioTrend() {
  const n = HISTORY.length;
  return HISTORY.map((month, i) => {
    const t = i / (n - 1);
    const cost = round2(14000 + (portfolioCost - 14000) * t);
    // A 2025 spring dip, otherwise drift towards today's gain.
    const dip = { '2025-03': -0.06, '2025-04': -0.12, '2025-05': -0.05 }[month] ?? 0;
    const gain =
      (portfolioValue / portfolioCost - 1) * t + dip + (noise('mkt', month) - 0.5) * 0.07;
    const last = i === n - 1;
    return {
      date: last ? TODAY : monthEnd(month),
      marketValue: last ? portfolioValue : round2(cost * (1 + gain)),
      costBasis: last ? portfolioCost : cost
    };
  });
}

function netWorthTrend() {
  const n = HISTORY.length;
  const portfolio = portfolioTrend();
  const cashNow = CASH.checking + CASH.savings + CASH.wallet;
  return HISTORY.map((month, i) => {
    const t = i / (n - 1);
    const last = i === n - 1;
    const cash = last
      ? cashNow
      : round2(9500 + (cashNow - 9500) * t + (noise('cash', month) - 0.5) * 1800);
    const assets = round2(cash + portfolio[i].marketValue);
    const card = last ? CARD_BALANCE : vary(620, 260, 'card', month);
    const loan = round2(14500 - (14500 - CAR_LOAN) * t);
    const liabilities = round2(card + loan);
    return {
      date: last ? TODAY : monthEnd(month),
      assets,
      liabilities,
      netWorth: round2(assets - liabilities)
    };
  });
}

// ── Accounts ──

type Node = {
  guid: string;
  name: string;
  accountType: string;
  code: string | null;
  description: string | null;
  placeholder: boolean;
  commodity: string | null;
  commodityGuid: string | null;
  balance: number;
  txCount: number;
  children: Node[];
};

/** Postings per account over the book's 46 months; monthly bills default to 46. */
const TX_COUNTS: Record<string, number> = {
  'acct-checking': 1214,
  'acct-savings': 58,
  'acct-wallet': 96,
  'acct-card': 846,
  'acct-vwrl': 41,
  'acct-emim': 23,
  'acct-aggh': 19,
  'acct-asml': 4,
  'inc-dividends': 15,
  'exp-utilities': 92,
  'exp-groceries': 548,
  'exp-dining': 263,
  'exp-fuel': 121,
  'exp-travel': 24,
  'exp-shopping': 152,
  'exp-health': 31,
  'exp-subscriptions': 138,
  'exp-events': 40,
  'exp-gifts': 28
};

function node(
  guid: string,
  name: string,
  accountType: string,
  balance: number,
  extra: Partial<Node> = {}
): Node {
  return {
    guid,
    name,
    accountType,
    code: null,
    description: null,
    placeholder: false,
    commodity: CURRENCY,
    commodityGuid: 'cmdty-eur',
    balance,
    txCount: TX_COUNTS[guid] ?? 46,
    children: [],
    ...extra
  };
}

function parent(guid: string, name: string, accountType: string, children: Node[]): Node {
  return node(guid, name, accountType, sum(children.map((c) => c.balance)), {
    placeholder: true,
    txCount: 0,
    children
  });
}

const lifetime = (fn: (month: string) => number) => sum(HISTORY.map(fn));

function accountTrees(): Node[] {
  const assets = parent('acct-assets', 'Assets', 'ASSET', [
    parent('acct-current', 'Current accounts', 'BANK', [
      node('acct-checking', 'Checking', 'BANK', CASH.checking),
      node('acct-savings', 'Savings', 'BANK', CASH.savings),
      node('acct-wallet', 'Wallet', 'CASH', CASH.wallet)
    ]),
    parent(
      'acct-brokerage',
      'Brokerage',
      'ASSET',
      HOLDINGS.map((h) =>
        node(`acct-${h.mnemonic.toLowerCase()}`, h.mnemonic, 'STOCK', round2(h.shares * h.price), {
          description: h.fullName,
          commodity: h.mnemonic,
          commodityGuid: h.guid
        })
      )
    )
  ]);
  const liabilities = parent('acct-liabilities', 'Liabilities', 'LIABILITY', [
    node('acct-card', 'Credit card', 'CREDIT', CARD_BALANCE),
    node('acct-car-loan', 'Car loan', 'LIABILITY', CAR_LOAN)
  ]);
  const incomeTree = parent('acct-income', 'Income', 'INCOME', [
    node('inc-salary', 'Salary', 'INCOME', lifetime(salary)),
    node('inc-interest', 'Interest', 'INCOME', lifetime(interest)),
    node('inc-dividends', 'Dividends', 'INCOME', lifetime(dividends))
  ]);
  const expenseTree = parent(
    'acct-expenses',
    'Expenses',
    'EXPENSE',
    EXPENSES.map((c) =>
      c.leaves.length === 1
        ? node(
            c.guid,
            c.name,
            'EXPENSE',
            lifetime((m) => leafSpend(c.leaves[0], m))
          )
        : parent(
            c.guid,
            c.name,
            'EXPENSE',
            c.leaves.map((l) =>
              node(
                l.guid,
                l.name,
                'EXPENSE',
                lifetime((m) => leafSpend(l, m))
              )
            )
          )
    )
  );
  const equity = parent('acct-equity', 'Equity', 'EQUITY', [
    node('acct-opening', 'Opening balances', 'EQUITY', 0, { txCount: 6 })
  ]);
  return [assets, liabilities, incomeTree, expenseTree, equity];
}

function accountsResult() {
  const trees = accountTrees();
  const count = (n: Node): number => 1 + n.children.reduce((a, c) => a + count(c), 0);
  const [assets, liabilities, incomeTree, expenseTree, equity] = trees;
  const unrealized = round2(portfolioValue - portfolioCost);
  const retained = round2(incomeTree.balance - expenseTree.balance);
  const byType = Object.fromEntries(
    [assets, liabilities, incomeTree, expenseTree, equity].map((t) => [
      t.accountType === 'CREDIT' ? 'LIABILITY' : t.accountType,
      { count: count(t), totalBalance: t.balance }
    ])
  );
  return {
    trees,
    byType,
    totalAccounts: trees.reduce((a, t) => a + count(t), 0),
    reportCurrency: CURRENCY,
    retainedEarnings: retained,
    unrealizedGains: unrealized,
    // Opening balances absorb the difference in a real book; the demo stays quiet.
    balanceCheck: 0,
    missingFx: [],
    missingPrices: []
  };
}

// ── Transactions ──

type Split = { guid: string; accountGuid: string; accountName: string; amount: number };
type Tx = {
  guid: string;
  postDate: string;
  description: string;
  num: null;
  notes: null;
  splits: Split[];
  totalAmount: number;
  tags: { guid: string; name: string }[];
  payee: { guid: string; name: string } | null;
};

const PAYEES: Record<string, string> = {
  employer: 'Northwind Studio',
  landlord: 'Harbour Lane Lettings',
  grocer: 'Daily Greens Market',
  bakery: 'Corner Bakery',
  bistro: 'Bistro Lume',
  fuel: 'Ridgeway Fuel',
  transit: 'Metro Transit',
  power: 'Brightwater Energy',
  internet: 'Fibrewave',
  streaming: 'Streamly',
  insurer: 'Keystone Insurance',
  pharmacy: 'Elm Street Pharmacy',
  books: 'Paperleaf Books'
};
const payee = (key: string) => ({ guid: `payee-${key}`, name: PAYEES[key] });

const TAGS = [
  { guid: 'tag-household', name: 'household' },
  { guid: 'tag-commute', name: 'commute' },
  { guid: 'tag-deductible', name: 'tax-deductible' },
  { guid: 'tag-autumn-trip', name: 'autumn-trip' }
];
const tag = (name: string) => TAGS.filter((t) => t.name === name);

const ACCOUNT_NAMES: Record<string, string> = {
  'acct-checking': 'Checking',
  'acct-savings': 'Savings',
  'acct-card': 'Credit card',
  'acct-vwrl': 'VWRL',
  'inc-salary': 'Salary',
  'inc-interest': 'Interest',
  ...Object.fromEntries(LEAVES.map((l) => [l.guid, l.name]))
};

let txSeq = 0;
function tx(
  postDate: string,
  description: string,
  from: string,
  to: string,
  amount: number,
  extra: Partial<Pick<Tx, 'tags' | 'payee'>> = {}
): Tx {
  const guid = `tx-${String(++txSeq).padStart(4, '0')}`;
  // `from` pays `to`; income flows from an income account into an asset.
  const inflow = from.startsWith('inc-');
  return {
    guid,
    postDate,
    description,
    num: null,
    notes: null,
    splits: [
      { guid: `${guid}-a`, accountGuid: from, accountName: ACCOUNT_NAMES[from], amount: -amount },
      { guid: `${guid}-b`, accountGuid: to, accountName: ACCOUNT_NAMES[to], amount }
    ],
    totalAmount: inflow ? amount : -amount,
    tags: [],
    payee: null,
    ...extra
  };
}

type TxArgs = Parameters<typeof tx> extends [string, ...infer Rest] ? Rest : never;

/** The last two months of activity, newest first. */
function recentTransactions(): Tx[] {
  const rows: Tx[] = [];
  for (const month of ['2026-09', '2026-10']) {
    const day = (d: number) => `${month}-${pad(d)}`;
    const visible = (d: number) => day(d) <= TODAY;
    const push = (d: number, t: TxArgs) => {
      if (visible(d)) rows.push(tx(day(d), ...t));
    };
    push(1, ['Rent', 'acct-checking', 'exp-rent', 1350, { payee: payee('landlord') }]);
    push(1, [
      'Fibre internet',
      'acct-checking',
      'exp-internet',
      39.9,
      { payee: payee('internet') }
    ]);
    push(1, [
      'Monthly pass',
      'acct-checking',
      'exp-transit',
      49,
      { payee: payee('transit'), tags: tag('commute') }
    ]);
    push(2, [
      'Home and car insurance',
      'acct-checking',
      'exp-insurance',
      68.4,
      { payee: payee('insurer') }
    ]);
    push(3, [
      'Streaming plan',
      'acct-card',
      'exp-subscriptions',
      12.99,
      { payee: payee('streaming') }
    ]);
    push(5, [
      'Electricity and heating',
      'acct-checking',
      'exp-utilities',
      round2(leafSpend(LEAVES[1], month) * 0.9),
      { payee: payee('power'), tags: tag('household') }
    ]);
    for (const [d, amt] of [
      [4, 86.35],
      [8, 112.7],
      [11, 64.18],
      [15, 131.42],
      [18, 92.05],
      [22, 118.9],
      [26, 74.6],
      [29, 101.33]
    ] as const) {
      push(d, [
        'Weekly groceries',
        'acct-card',
        'exp-groceries',
        vary(amt, 6, 'g', month, d),
        { payee: payee('grocer'), tags: tag('household') }
      ]);
    }
    for (const d of [6, 13, 20, 27]) {
      push(d, [
        'Bread and pastries',
        'acct-card',
        'exp-groceries',
        vary(9.8, 3, 'b', month, d),
        { payee: payee('bakery') }
      ]);
    }
    push(9, [
      'Dinner with friends',
      'acct-card',
      'exp-dining',
      vary(68, 12, 'd1', month),
      { payee: payee('bistro') }
    ]);
    push(19, [
      'Lunch',
      'acct-card',
      'exp-dining',
      vary(24, 6, 'd2', month),
      { payee: payee('bistro') }
    ]);
    push(24, [
      'Birthday dinner',
      'acct-card',
      'exp-dining',
      vary(92, 14, 'd3', month),
      { payee: payee('bistro') }
    ]);
    push(7, [
      'Fuel',
      'acct-card',
      'exp-fuel',
      vary(58, 8, 'f1', month),
      { payee: payee('fuel'), tags: tag('commute') }
    ]);
    push(21, [
      'Fuel',
      'acct-card',
      'exp-fuel',
      vary(61, 8, 'f2', month),
      { payee: payee('fuel'), tags: tag('commute') }
    ]);
    push(12, [
      'Prescription',
      'acct-card',
      'exp-health',
      18.45,
      { payee: payee('pharmacy'), tags: tag('tax-deductible') }
    ]);
    push(16, ['Books', 'acct-card', 'exp-shopping', 34.9, { payee: payee('books') }]);
    push(15, ['Salary', 'inc-salary', 'acct-checking', SALARY, { payee: payee('employer') }]);
    push(16, ['Monthly investment', 'acct-checking', 'acct-vwrl', 1000]);
    push(17, ['Pay credit card', 'acct-checking', 'acct-card', vary(840, 60, 'cc', month)]);
    push(30, ['Savings interest', 'inc-interest', 'acct-savings', interest(month)]);
  }
  // A weekend away tagged for the trip.
  rows.push(
    tx('2026-10-10', 'Train tickets', 'acct-card', 'exp-travel', 84.4, {
      tags: tag('autumn-trip')
    }),
    tx('2026-10-11', 'Guesthouse, two nights', 'acct-card', 'exp-travel', 236, {
      tags: tag('autumn-trip')
    })
  );
  return rows.sort((a, b) =>
    a.postDate === b.postDate ? b.guid.localeCompare(a.guid) : b.postDate.localeCompare(a.postDate)
  );
}

const TOTAL_TRANSACTIONS = 3418;

function transactionsPage(pageSize: number) {
  const rows = recentTransactions().slice(0, pageSize);
  return {
    transactions: rows,
    total: TOTAL_TRANSACTIONS,
    page: 1,
    pageSize,
    totalPages: Math.ceil(TOTAL_TRANSACTIONS / pageSize)
  };
}

/** A year of daily activity: busier on weekends, quiet on some days. */
function heatmap() {
  const out: { date: string; count: number }[] = [];
  const end = new Date(`${TODAY}T12:00:00`);
  for (let i = 364; i >= 0; i--) {
    const d = new Date(end);
    d.setDate(end.getDate() - i);
    const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
    const weekend = d.getDay() === 0 || d.getDay() === 6;
    const count = Math.floor(noise('heat', date) * (weekend ? 7 : 5));
    if (count > 0) out.push({ date, count });
  }
  return out;
}

const usage = (key: string, n: number) => ({
  transactionCount: n,
  lastUsed: `2026-10-${pad(Math.max(1, Math.round(noise('used', key) * 19)))}`
});

// ── Reports ──

const SPENDING_MONTHS = monthsBetween('2025-10', '2026-09');
const PREV_MONTHS = monthsBetween('2024-10', '2025-09');

function spendingReport() {
  const categories = EXPENSES.map((c) => {
    const monthly = SPENDING_MONTHS.map((m) => categorySpend(c, m));
    const total = sum(monthly);
    const previousTotal = sum(PREV_MONTHS.map((m) => categorySpend(c, m) * 0.96));
    return {
      accountGuid: c.guid,
      accountName: c.name,
      hasChildren: c.leaves.length > 1,
      ownSplits: false,
      monthly,
      total,
      previousTotal,
      changePct: previousTotal ? round2(((total - previousTotal) / previousTotal) * 100) : null
    };
  }).sort((a, b) => b.total - a.total);
  const total = sum(categories.map((c) => c.total));
  const totalPrev = sum(categories.map((c) => c.previousTotal));
  return {
    dateFrom: '2025-10-01',
    dateTo: '2026-09-30',
    prevDateFrom: '2024-10-01',
    prevDateTo: '2025-09-30',
    reportCurrency: CURRENCY,
    path: [],
    months: SPENDING_MONTHS,
    categories,
    total,
    totalPrev,
    changePct: round2(((total - totalPrev) / totalPrev) * 100),
    missingFx: []
  };
}

function pnlThisMonth() {
  const prev = '2026-09';
  const incomeNow = income(CURRENT_MONTH);
  const expensesNow = totalSpend(CURRENT_MONTH);
  const incomePrev = income(prev);
  const expensesPrev = totalSpend(prev);
  const row = (accountGuid: string, accountName: string, current: number, previous: number) => ({
    accountGuid,
    accountName,
    accountCode: null,
    indent: 0,
    currentAmount: current,
    previousAmount: previous,
    changePct: previous ? round2(((current - previous) / previous) * 100) : null
  });
  return {
    dateFrom: `${CURRENT_MONTH}-01`,
    dateTo: TODAY,
    prevDateFrom: '2026-09-01',
    prevDateTo: '2026-09-20',
    reportCurrency: CURRENCY,
    incomeRows: [row('inc-salary', 'Salary', SALARY, SALARY)],
    incomeRootGuid: 'acct-income',
    totalIncome: incomeNow,
    totalIncomePrev: incomePrev,
    expenseRows: EXPENSES.map((c) =>
      row(c.guid, c.name, categorySpend(c, CURRENT_MONTH), categorySpend(c, prev))
    ),
    expenseRootGuid: 'acct-expenses',
    totalExpenses: expensesNow,
    totalExpensesPrev: expensesPrev,
    netIncome: round2(incomeNow - expensesNow),
    netIncomePrev: round2(incomePrev - expensesPrev),
    netIncomeChangePct: null,
    missingFx: []
  };
}

function pnlTrend() {
  return monthsBetween('2025-11', CURRENT_MONTH).map((month) => ({
    month,
    income: income(month),
    expenses: totalSpend(month)
  }));
}

// ── Budgets ──

const BUDGETS: Record<string, number> = {
  'exp-housing': 18600,
  'exp-groceries': 6600,
  'exp-dining': 2100,
  'exp-transport': 2100,
  'exp-travel': 3500,
  'exp-shopping': 2400,
  'exp-health': 900,
  'exp-entertainment': 900
};

function budgetRows() {
  const year = CURRENT_MONTH.slice(0, 4);
  const elapsed = month0(CURRENT_MONTH);
  return EXPENSES.filter((c) => BUDGETS[c.guid]).map((c) => {
    const budgetAmount = BUDGETS[c.guid];
    const actualAmount = sum(
      monthsBetween(`${year}-01`, CURRENT_MONTH).map((m) => categorySpend(c, m))
    );
    const expectedYtd = round2((budgetAmount * elapsed) / 12);
    return {
      guid: `budget-${c.guid}`,
      accountGuid: c.guid,
      accountName: c.name,
      accountCode: null,
      accountType: 'EXPENSE',
      budgetAmount,
      actualAmount,
      remaining: round2(budgetAmount - actualAmount),
      percentage: round2((actualAmount / budgetAmount) * 100),
      rolloverBalance: 0,
      expectedYtd,
      pacingPercentage: round2((actualAmount / expectedYtd) * 100),
      delta: round2(expectedYtd - actualAmount),
      elapsedMonths: elapsed,
      effectiveTotal: budgetAmount
    };
  });
}

// ── Mocks ──

const netWorth = netWorthTrend();
const valueTrend = portfolioTrend();
const holdings = portfolioHoldings();
const totalUnrealized = sum(holdings.map((h) => h.unrealizedGain ?? 0));
const totalRealized = sum(holdings.map((h) => h.realizedGain));
const totalDividends = sum(holdings.map((h) => h.dividends));

/** Every command the screenshotted pages call, with the demo book's answers. */
export const DEMO_BOOK: BackendMocks = {
  fetch_active_book: { path: '/home/alex/Documents/Household.spendo' },
  fetch_home_data: {
    hasData: true,
    summary: {
      currency: CURRENCY,
      totalAccounts: accountsResult().totalAccounts,
      totalTransactions: TOTAL_TRANSACTIONS,
      earliestDate: '2023-01-02',
      latestDate: TODAY,
      netWorth: netWorth[netWorth.length - 1].netWorth
    }
  },
  fetch_nav_badges: { dueSchedules: 1, unclearedTransactions: 0 },
  get_book_settings: {
    fiscalYearStartMonth: 1,
    defaultPnlPeriod: 'this_month',
    priceAutoRefresh: 'on_startup',
    exportCsvDelimiter: ',',
    exportDateFormat: 'iso',
    autoBackup: 'daily',
    transitAccount: '',
    costBasisMethod: 'fifo',
    taxDeductibleTag: 'tax-deductible'
  },
  fetch_net_worth_trend: netWorth,
  fetch_pnl_report: pnlThisMonth(),
  fetch_pnl_trend: pnlTrend(),
  fetch_accounts: accountsResult(),
  fetch_tags: [
    { ...TAGS[0], ...usage('household', 412) },
    { ...TAGS[1], ...usage('commute', 286) },
    { ...TAGS[2], ...usage('deductible', 41) },
    { ...TAGS[3], ...usage('trip', 2) }
  ],
  fetch_payees: Object.keys(PAYEES).map((key, i) => ({
    ...payee(key),
    ...usage(key, 300 - i * 19)
  })),
  fetch_transaction_heatmap: heatmap(),
  fetch_spending_report: spendingReport(),
  fetch_budget_vs_actual: budgetRows(),
  fetch_budget_date_range: { earliestDate: '2023-01-02', latestDate: TODAY },
  fetch_portfolio_data: {
    totalMarketValue: portfolioValue,
    totalCostBasis: portfolioCost,
    totalGainLoss: totalUnrealized,
    totalOpenCostBasis: portfolioCost,
    totalUnrealizedGain: totalUnrealized,
    totalRealizedGain: totalRealized,
    totalDividends,
    totalReturn: round2(totalUnrealized + totalRealized + totalDividends),
    totalXirr: 8.1,
    lastRefreshed: `${TODAY}T08:12:00Z`,
    holdings,
    valueTrend
  },
  fetch_allocation_targets: []
};

/** `fetch_transactions` answers depend on the page size each screen asks for. */
export const transactionsFor = (pageSize: number) => transactionsPage(pageSize);
