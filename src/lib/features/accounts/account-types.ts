export const TYPE_CATEGORY: Record<string, string> = {
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
  EQUITY: 'EQUITY'
};

export const CATEGORY_ORDER = ['ASSET', 'LIABILITY', 'EQUITY', 'INCOME', 'EXPENSE'];

/** Category of an account type, falling back to the raw type when unknown. */
export function nodeCategory(accountType: string): string {
  return TYPE_CATEGORY[accountType] ?? accountType;
}

/** Singular display labels for categories (table headers, tree groupings). */
export const CATEGORY_LABEL_SINGULAR: Record<string, string> = {
  ASSET: 'Asset',
  LIABILITY: 'Liability',
  INCOME: 'Income',
  EXPENSE: 'Expense',
  EQUITY: 'Equity'
};

/** Plural display labels (KPI cards, section summaries). */
export const CATEGORY_LABEL_PLURAL: Record<string, string> = {
  ASSET: 'Assets',
  LIABILITY: 'Liabilities',
  INCOME: 'Income',
  EXPENSE: 'Expenses',
  EQUITY: 'Equity'
};

/** Per-category accent colors, shared by the balance charts (treemap, tree). */
export const CATEGORY_COLOR: Record<string, string> = {
  ASSET: 'var(--color-success)',
  LIABILITY: 'var(--color-destructive)',
  EQUITY: 'var(--color-warning)',
  INCOME: 'var(--color-chart-4)',
  EXPENSE: 'var(--color-chart-5)'
};

/** Per-account-type accent colors for the account hierarchy tree. */
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
  EQUITY: 'var(--color-yellow-500)'
};
