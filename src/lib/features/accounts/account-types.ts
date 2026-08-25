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
