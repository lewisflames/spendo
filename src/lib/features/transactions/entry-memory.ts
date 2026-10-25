/**
 * Best-effort entry-speed hints for the transaction form, kept in
 * localStorage: the accounts used last time and recently entered
 * transaction shapes ("recipes") keyed by description.
 */

const LAST_ACCOUNTS_KEY = 'tx-last-accounts';
const RECENT_RECIPES_KEY = 'tx-recent-recipes';
const MAX_ENTRIES = 20;

/** One remembered split of a recipe. */
interface RecipeSplit {
  accountGuid: string;
  amount: string;
  quantity: string;
  showQuantity: boolean;
  memo: string;
}

/** One remembered transaction shape, keyed by its description. */
export interface TxRecipe {
  description: string;
  /** Payee name used last time; absent in recipes saved before payees. */
  payee?: string;
  splits: RecipeSplit[];
}

function readStoredList<T>(key: string, isItem: (v: unknown) => v is T): T[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isItem) : [];
  } catch {
    return [];
  }
}

function writeStoredList<T>(key: string, values: T[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(values.slice(0, MAX_ENTRIES)));
  } catch {
    // Storage full or unavailable — entry speed hints are best-effort.
  }
}

function isString(v: unknown): v is string {
  return typeof v === 'string';
}

function isRecipe(v: unknown): v is TxRecipe {
  if (!v || typeof v !== 'object') return false;
  const r = v as { description?: unknown; splits?: unknown };
  return typeof r.description === 'string' && Array.isArray(r.splits);
}

export function readLastAccounts(): string[] {
  return readStoredList(LAST_ACCOUNTS_KEY, isString);
}

export function writeLastAccounts(guids: string[]): void {
  writeStoredList(LAST_ACCOUNTS_KEY, guids);
}

export function readRecipes(): TxRecipe[] {
  return readStoredList(RECENT_RECIPES_KEY, isRecipe);
}

export function writeRecipes(recipes: TxRecipe[]): void {
  writeStoredList(RECENT_RECIPES_KEY, recipes);
}
