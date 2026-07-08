export const TYPE_CATEGORY: Record<string, string> = {
  ASSET: 'ASSET',
  BANK: 'ASSET',
  CASH: 'ASSET',
  STOCK: 'ASSET',
  MUTUAL: 'ASSET',
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
