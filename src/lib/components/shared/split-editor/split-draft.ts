import type { SplitInput } from '$lib/tauri';

export interface SplitDraft {
  key: number;
  accountGuid: string;
  amount: string;
  quantity: string;
  showQuantity: boolean;
  memo: string;
}

export function makeSplitDraft(key: number): SplitDraft {
  return {
    key,
    accountGuid: '',
    amount: '',
    quantity: '',
    showQuantity: false,
    memo: ''
  };
}

/**
 * Parse a raw split amount or quantity. Accepts an optional sign, digits and
 * an optional single `.`/`,` decimal separator (`12.50`, `-3`, `1,5`); a `,`
 * is treated as the decimal separator. Rejects blanks, hex (`0x10`),
 * scientific notation (`1e3`) and malformed text so a mistyped amount can't
 * silently parse as a different number. Returns `null` when empty/invalid.
 */
export function parseAmount(raw: string): number | null {
  const t = raw.trim();
  if (!t) return null;
  if (!/^[+-]?(\d+)(?:([.,])(\d+))?$/.test(t)) return null;
  const n = Number(t.replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

export function draftFromSplit(
  split: {
    accountGuid: string;
    amount: number;
    quantity?: number | null;
    memo?: string | null;
  },
  key: number
): SplitDraft {
  const qty = split.quantity ?? null;
  const qtyDiffers =
    qty !== null && qty !== 0 && Math.abs(Math.abs(qty) - Math.abs(split.amount)) >= 1e-6;
  return {
    key,
    accountGuid: split.accountGuid,
    amount: String(split.amount),
    quantity: qtyDiffers && qty !== null ? String(qty) : '',
    showQuantity: qtyDiffers,
    memo: split.memo ?? ''
  };
}

export function splitBalance(splits: SplitDraft[]): number | null {
  let sum = 0;
  for (const s of splits) {
    const n = parseAmount(s.amount);
    if (n === null) return null;
    sum += n;
  }
  return sum;
}

export function isBalanced(balance: number | null): boolean {
  return balance !== null && Math.abs(balance) < 1e-6;
}

export function splitErrors(splits: SplitDraft[]): string[] {
  return splits.map((s, i) => {
    if (!s.accountGuid) return `Split ${i + 1}: pick an account`;
    if (parseAmount(s.amount) === null) return `Split ${i + 1}: enter a number`;
    if (s.showQuantity && s.quantity.trim() && parseAmount(s.quantity) === null)
      return `Split ${i + 1}: quantity must be a number`;
    return '';
  });
}

export function splitToInput(s: SplitDraft): SplitInput {
  return {
    accountGuid: s.accountGuid,
    amount: parseAmount(s.amount) ?? 0,
    quantity: s.showQuantity && s.quantity.trim() ? parseAmount(s.quantity) : null,
    memo: s.memo.trim() ? s.memo.trim() : null
  };
}
