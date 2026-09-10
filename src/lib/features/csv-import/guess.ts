/** Pure column-guessing for the CSV mapping step (unit-tested, no Tauri). */

interface GuessedMapping {
  dateCol: number;
  descCol: number;
  amountCol: number | null;
  debitCol: number | null;
  creditCol: number | null;
  usePair: boolean;
}

function findCol(headers: string[], keys: string[]): number | null {
  const lower = headers.map((h) => h.toLowerCase());
  for (const key of keys) {
    const idx = lower.findIndex((h) => h.includes(key));
    if (idx >= 0) return idx;
  }
  return null;
}

/**
 * Guess date/description/amount columns from headers (or the first sample
 * row when the file has no header). Debit+Credit wins when both are found;
 * otherwise a single Amount column. Falls back to 0/1/last-column positions.
 */
export function guessMapping(headers: string[], columnCount: number): GuessedMapping {
  const dateCol = findCol(headers, ['date', 'datum', 'posted', 'value date', 'booking']) ?? 0;
  const descCol =
    findCol(headers, ['desc', 'narrat', 'memo', 'payee', 'details', 'text', 'reference']) ??
    (columnCount > 1 ? 1 : 0);
  const debitCol = findCol(headers, ['debit', 'soll', 'paid out', 'withdrawal', 'spent']);
  const creditCol = findCol(headers, ['credit', 'haben', 'paid in', 'deposit', 'received']);
  if (debitCol !== null && creditCol !== null && debitCol !== creditCol) {
    return { dateCol, descCol, amountCol: null, debitCol, creditCol, usePair: true };
  }
  const amountCol =
    findCol(headers, ['amount', 'sum', 'betrag', 'value', 'total']) ??
    (columnCount > 0 ? columnCount - 1 : 0);
  return { dateCol, descCol, amountCol, debitCol: null, creditCol: null, usePair: false };
}
