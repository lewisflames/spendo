/** Pure statement math for the reconcile page (tested, no Svelte state). */

/** Half-cent tolerance matching the backend `BALANCE_EPS`. */
const RECONCILE_EPS = 0.005;

/** Cached locale group/decimal separators for the amount parser. */
const separatorCache = new Map<string, { group: string; decimal: string }>();

function localeSeparators(locale: string): { group: string; decimal: string } {
  const cached = separatorCache.get(locale);
  if (cached) return cached;
  let group = ',';
  let decimal = '.';
  try {
    for (const part of new Intl.NumberFormat(locale).formatToParts(1234.5)) {
      if (part.type === 'group') group = part.value;
      else if (part.type === 'decimal') decimal = part.value;
    }
  } catch {
    // Unknown locale tag: keep the en-US fallback.
  }
  const result = { group, decimal };
  separatorCache.set(locale, result);
  return result;
}

/** Whether `text` uses `group` only as thousands grouping (`1,234`, `+1.234`). */
function validGrouping(text: string, group: string): boolean {
  const parts = text.split(group);
  if (parts.length < 2) return true;
  if (!/^[+-]?\d{1,3}$/.test(parts[0])) return false;
  return parts.slice(1).every((part) => /^\d{3}$/.test(part));
}

/** Sum of checked candidate amounts (signed display amounts). */
export function clearedTotal(amounts: number[]): number {
  return amounts.reduce((sum, n) => sum + n, 0);
}

/** Expected statement total from the reconciled opening balance + cleared. */
export function targetBalance(startingBalance: number, cleared: number): number {
  return startingBalance + cleared;
}

/** `ending - target`; zero means the statement balances. */
export function statementDifference(endingBalance: number, target: number): number {
  return endingBalance - target;
}

/** Whether the statement balances within half-cent tolerance. */
export function isBalanced(difference: number, eps = RECONCILE_EPS): boolean {
  return Math.abs(difference) <= eps;
}

/**
 * Parse a statement ending-balance input. Accepts plain decimals (`1234.56`,
 * `-12,5`), thousands-grouped values (`1,000.50`, `1.000,50`, `1 234,56`) and
 * both sign forms. Grouping is validated (three-digit groups), and a lone
 * separator with exactly three digits is resolved with the locale's
 * group/decimal separators — ambiguous input is rejected rather than guessed,
 * so Finish can’t silently round a mistyped value. Returns `null` when
 * empty/invalid.
 */
export function parseStatementAmount(raw: string, locale = 'en'): number | null {
  // Strip spacing and apostrophes used as grouping symbols (`1 234,56`,
  // `1'234.56`).
  const text = raw.trim().replace(/[\s\u00a0\u202f']/g, '');
  if (text === '' || !/^[+-]?[\d.,]+$/.test(text)) return null;
  const { group, decimal } = localeSeparators(locale);
  const hasDot = text.includes('.');
  const hasComma = text.includes(',');
  let value: string;

  if (hasDot && hasComma) {
    // Both present: the rightmost separator is decimal, the other groups.
    const decimalSep = text.lastIndexOf('.') > text.lastIndexOf(',') ? '.' : ',';
    const groupSep = decimalSep === '.' ? ',' : '.';
    const cut = text.lastIndexOf(decimalSep);
    const intPart = text.slice(0, cut);
    const fraction = text.slice(cut + 1);
    if (!validGrouping(intPart, groupSep) || !/^\d+$/.test(fraction)) return null;
    value = `${intPart.split(groupSep).join('')}.${fraction}`;
  } else if (hasDot || hasComma) {
    const sep = hasDot ? '.' : ',';
    const occurrences = text.split(sep).length - 1;
    if (occurrences > 1) {
      // A repeated separator can only be grouping (`1.234.567`).
      if (!validGrouping(text, sep)) return null;
      value = text.split(sep).join('');
    } else if (sep === decimal) {
      value = text;
    } else {
      const fraction = text.slice(text.lastIndexOf(sep) + 1);
      if (fraction.length === 3) {
        // A single three-digit group is ambiguous: only grouping with the
        // locale's group separator is accepted (`1,000` in en-US).
        if (sep !== group || !validGrouping(text, sep)) return null;
        value = text.split(sep).join('');
      } else {
        // Can't be valid grouping (groups are three digits), so read it as a
        // decimal even when the character matches the locale's group symbol
        // (de-DE `12.34`).
        value = text;
      }
    }
  } else {
    value = text;
  }

  value = value.replace(',', '.');
  if (!/^[+-]?\d+(\.\d+)?$/.test(value)) return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

/** Tri-state of the table header checkbox for the visible rows. */
export function selectionState(
  checked: Pick<Set<string>, 'has'>,
  rows: readonly { splitGuid: string }[]
): 'all' | 'some' | 'none' {
  if (rows.length === 0) return 'none';
  let count = 0;
  for (const r of rows) if (checked.has(r.splitGuid)) count += 1;
  if (count === 0) return 'none';
  return count === rows.length ? 'all' : 'some';
}

/** Split guids from `fromGuid` to `toGuid` inclusive, in current row order.
 * Falls back to just `toGuid` when the anchor is missing or filtered out. */
export function rangeSplitGuids(
  rows: readonly { splitGuid: string }[],
  fromGuid: string | null,
  toGuid: string
): string[] {
  const to = rows.findIndex((r) => r.splitGuid === toGuid);
  if (to === -1) return [];
  const from = fromGuid === null ? -1 : rows.findIndex((r) => r.splitGuid === fromGuid);
  if (from === -1) return [toGuid];
  const [start, end] = from <= to ? [from, to] : [to, from];
  return rows.slice(start, end + 1).map((r) => r.splitGuid);
}

/**
 * Guids of candidates sharing a post date and amount with another row — a hint
 * for likely duplicate imports (identical real purchases happen too).
 */
export function duplicateCandidateGuids(
  rows: readonly { splitGuid: string; postDate: string; amount: number }[]
): Set<string> {
  const byKey = new Map<string, string[]>();
  for (const row of rows) {
    const key = `${row.postDate}|${Math.round(row.amount * 100)}`;
    const guids = byKey.get(key);
    if (guids) guids.push(row.splitGuid);
    else byKey.set(key, [row.splitGuid]);
  }
  const out = new Set<string>();
  for (const guids of byKey.values()) {
    if (guids.length > 1) for (const guid of guids) out.add(guid);
  }
  return out;
}

/** Whether the reconcile workflow supports an account (mirrors backend `RECONCILABLE_TYPES`). */
export function isReconcilable(accountType: string, placeholder: boolean): boolean {
  if (placeholder) return false;
  return (
    accountType === 'ASSET' ||
    accountType === 'BANK' ||
    accountType === 'CASH' ||
    accountType === 'CURRENCY' ||
    accountType === 'RECEIVABLE' ||
    accountType === 'LIABILITY' ||
    accountType === 'CREDIT' ||
    accountType === 'PAYABLE'
  );
}
