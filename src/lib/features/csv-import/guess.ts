/** Pure column-guessing for the CSV mapping step (unit-tested, no Tauri). */

interface GuessedMapping {
  dateCol: number;
  descCol: number;
  amountCol: number | null;
  debitCol: number | null;
  creditCol: number | null;
  numCol: number | null;
  notesCol: number | null;
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

/** Like {@link findCol}, but never steals a column another field already uses. */
function findFreeCol(headers: string[], keys: string[], taken: number[]): number | null {
  const idx = findCol(headers, keys);
  return idx !== null && !taken.includes(idx) ? idx : null;
}

/**
 * Guess date/description/amount columns from headers (or the first sample
 * row when the file has no header). Debit+Credit wins when both are found;
 * otherwise a single Amount column. Reference/notes columns are guessed only
 * when they don’t collide with the fields above. Falls back to 0/1/last-column
 * positions.
 */
export function guessMapping(headers: string[], columnCount: number): GuessedMapping {
  const dateCol = findCol(headers, ['date', 'datum', 'posted', 'value date', 'booking']) ?? 0;
  const descCol =
    findCol(headers, ['desc', 'narrat', 'memo', 'payee', 'details', 'text', 'reference']) ??
    (columnCount > 1 ? 1 : 0);
  const debitCol = findCol(headers, ['debit', 'soll', 'paid out', 'withdrawal', 'spent']);
  const creditCol = findCol(headers, ['credit', 'haben', 'paid in', 'deposit', 'received']);
  const pair = debitCol !== null && creditCol !== null && debitCol !== creditCol;
  const amountCol = pair
    ? null
    : (findCol(headers, ['amount', 'sum', 'betrag', 'value', 'total']) ??
      (columnCount > 0 ? columnCount - 1 : 0));

  const taken = [dateCol, descCol, amountCol, debitCol, creditCol].filter(
    (c): c is number => c !== null
  );
  const numCol = findFreeCol(
    headers,
    ['reference', 'ref', 'number', 'transaction id', 'cheque', 'check no'],
    taken
  );
  if (numCol !== null) taken.push(numCol);
  const notesCol = findFreeCol(headers, ['note', 'notes', 'memo', 'details'], taken);

  return {
    dateCol,
    descCol,
    amountCol,
    debitCol: pair ? debitCol : null,
    creditCol: pair ? creditCol : null,
    numCol,
    notesCol,
    usePair: pair
  };
}
