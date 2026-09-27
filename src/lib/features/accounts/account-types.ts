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

export const CATEGORY_ORDER = ['ASSET', 'LIABILITY', 'EQUITY', 'INCOME', 'EXPENSE'];

/** Creatable account types (mirrors backend `ACCOUNT_TYPES`; ROOT is internal). */
export const ACCOUNT_TYPES = [
  'ASSET',
  'BANK',
  'CASH',
  'STOCK',
  'MUTUAL',
  'FUND',
  'CURRENCY',
  'RECEIVABLE',
  'LIABILITY',
  'CREDIT',
  'PAYABLE',
  'INCOME',
  'EXPENSE',
  'EQUITY',
  'TRADING'
];

/** Category of an account type, falling back to the raw type when unknown. */
export function nodeCategory(accountType: string): string {
  return TYPE_TO_CATEGORY[accountType] ?? accountType;
}

/** Plural display labels (KPI cards, section summaries). */
export const CATEGORY_LABEL_PLURAL: Record<string, string> = {
  ASSET: 'Assets',
  LIABILITY: 'Liabilities',
  INCOME: 'Income',
  EXPENSE: 'Expenses',
  EQUITY: 'Equity'
};

/** Per-account-type accent colors for charts. */
export const TYPE_COLOR: Record<string, string> = {
  ASSET: 'var(--color-slate-500)',
  BANK: 'var(--color-blue-500)',
  CASH: 'var(--color-emerald-500)',
  STOCK: 'var(--color-violet-500)',
  MUTUAL: 'var(--color-purple-500)',
  FUND: 'var(--color-fuchsia-500)',
  CURRENCY: 'var(--color-sky-500)',
  RECEIVABLE: 'var(--color-teal-500)',
  LIABILITY: 'var(--color-red-500)',
  CREDIT: 'var(--color-rose-500)',
  PAYABLE: 'var(--color-orange-500)',
  INCOME: 'var(--color-green-500)',
  EXPENSE: 'var(--color-amber-500)',
  EQUITY: 'var(--color-yellow-500)',
  TRADING: 'var(--color-lime-500)'
};
