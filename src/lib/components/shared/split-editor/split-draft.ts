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

/** Account types whose splits carry a share quantity distinct from the amount. */
export const INVESTMENT_ACCOUNT_TYPES = new Set(['STOCK', 'MUTUAL', 'FUND']);

export function isInvestmentAccountType(t: string | null | undefined): boolean {
  return !!t && INVESTMENT_ACCOUNT_TYPES.has(t.toUpperCase());
}

const CURRENCY_SYMBOLS = new Set(['$', '€', '£', '¥', '₹', '₩', '₪', '₽', '₺']);

function stripCurrencyEnds(s: string): string {
  let out = s.trim();
  while (out.length > 0 && CURRENCY_SYMBOLS.has(out[0])) out = out.slice(1).trim();
  while (out.length > 0 && CURRENCY_SYMBOLS.has(out[out.length - 1])) out = out.slice(0, -1).trim();
  return out;
}

/**
 * Normalize raw input to an evaluable expression: strips currency symbols,
 * spaces/apostrophes used as thousands separators and resolves `,` handling.
 * Returns `null` when the shape is not a number or simple arithmetic.
 */
function normalizeAmountInput(raw: string): string | null {
  let t = stripCurrencyEnds(raw);
  if (!t) return null;
  if (t.length > 64) return null;
  // Spaces, apostrophes and no-break spaces are thousands separators.
  t = t.replace(/[\s'’‘`\u00a0\u202f]/g, '');
  if (!t) return null;
  const hasDot = t.includes('.');
  const hasComma = t.includes(',');
  if (hasDot && hasComma) {
    // `1,000.50` or `1.000,50`: the last separator is the decimal one.
    const lastDot = t.lastIndexOf('.');
    const lastComma = t.lastIndexOf(',');
    if (lastComma > lastDot) {
      t = t.replace(/\./g, '').replace(',', '.');
    } else {
      t = t.replace(/,/g, '');
    }
  } else if (hasComma) {
    // Thousands (`1,000`, `12,345,678`) vs decimal comma (`1,5`).
    if (/^[+-]?\d{1,3}(,\d{3})+$/.test(t)) {
      t = t.replace(/,/g, '');
    } else if (/^[+-]?\d+,\d+$/.test(t)) {
      t = t.replace(',', '.');
    } else {
      return null;
    }
  }
  // Only digits, decimal points, operators and parens survive normalization.
  if (!/^[0-9.+\-*/()]+$/.test(t)) return null;
  if (/[0-9.]e|[eE]/.test(t)) return null;
  return t;
}

function tokenize(expr: string): (number | string)[] | null {
  const tokens: (number | string)[] = [];
  let i = 0;
  const isOp = (c: string) => c === '+' || c === '-' || c === '*' || c === '/';
  // At a value position a leading `-`/`+` (possibly repeated) is a sign.
  let expectValue = true;
  while (i < expr.length) {
    const c = expr[i];
    if (c === '(') {
      tokens.push('(');
      i += 1;
      expectValue = true;
      continue;
    }
    if (c === ')') {
      tokens.push(')');
      i += 1;
      expectValue = false;
      continue;
    }
    if (isOp(c)) {
      if (expectValue && (c === '+' || c === '-')) {
        // Parse a run of signs (`--5` → `5`, `+-5` → `-5`).
        let sign = 1;
        while (i < expr.length && (expr[i] === '+' || expr[i] === '-')) {
          if (expr[i] === '-') sign *= -1;
          i += 1;
        }
        if (expr[i] === '(') {
          // Unary sign before a group: `-(a+b)` → `-1 * (a+b)`.
          if (sign === -1) tokens.push(-1, '*');
          continue;
        }
        let num = '';
        while (i < expr.length && /[0-9.]/.test(expr[i])) num += expr[i++];
        if (!/^\d+(\.\d+)?$/.test(num)) return null;
        const n = Number(num) * sign;
        if (!Number.isFinite(n)) return null;
        tokens.push(n);
        expectValue = false;
        continue;
      }
      tokens.push(c);
      i += 1;
      expectValue = true;
      continue;
    }
    if (/[0-9.]/.test(c)) {
      let num = '';
      while (i < expr.length && /[0-9.]/.test(expr[i])) num += expr[i++];
      if (!/^\d+(\.\d+)?$/.test(num)) return null;
      const n = Number(num);
      if (!Number.isFinite(n)) return null;
      tokens.push(n);
      expectValue = false;
      continue;
    }
    return null;
  }
  return tokens;
}

function evaluateTokens(tokens: (number | string)[]): number | null {
  // Shunting-yard to RPN, then evaluate. No functions or variables.
  const prec = (op: string) => (op === '+' || op === '-' ? 1 : 2);
  const output: (number | string)[] = [];
  const ops: string[] = [];
  for (const t of tokens) {
    if (typeof t === 'number') {
      output.push(t);
    } else if (t === '(') {
      ops.push(t);
    } else if (t === ')') {
      let found = false;
      while (ops.length > 0) {
        const op = ops.pop() as string;
        if (op === '(') {
          found = true;
          break;
        }
        output.push(op);
      }
      if (!found) return null;
    } else if (t === '+' || t === '-' || t === '*' || t === '/') {
      while (
        ops.length > 0 &&
        ops[ops.length - 1] !== '(' &&
        prec(ops[ops.length - 1]) >= prec(t)
      ) {
        output.push(ops.pop() as string);
      }
      ops.push(t);
    } else {
      return null;
    }
  }
  while (ops.length > 0) {
    const op = ops.pop() as string;
    if (op === '(') return null;
    output.push(op);
  }
  const stack: number[] = [];
  for (const t of output) {
    if (typeof t === 'number') {
      stack.push(t);
      continue;
    }
    if (stack.length < 2) return null;
    const b = stack.pop() as number;
    const a = stack.pop() as number;
    let r: number;
    switch (t) {
      case '+':
        r = a + b;
        break;
      case '-':
        r = a - b;
        break;
      case '*':
        r = a * b;
        break;
      case '/':
        if (b === 0) return null;
        r = a / b;
        break;
      default:
        return null;
    }
    if (!Number.isFinite(r)) return null;
    stack.push(r);
  }
  if (stack.length !== 1) return null;
  return stack[0];
}

/**
 * Parse a raw split amount or quantity. Accepts thousands separators
 * (`1,000.50`, `1 000`, `1'000`), leading currency symbols (`$12.50`) and
 * simple arithmetic (`10+5`, `100/4`, `(10+2)*3`). A `,` acts as the decimal
 * separator only when it is the sole separator (`1,5`); when both `.` and
 * `,` are present the last one wins. Rejects blanks, hex (`0x10`),
 * scientific notation (`1e3`) and malformed text so a mistyped amount can't
 * silently parse as a different number. Returns `null` when empty/invalid.
 */
export function parseAmount(raw: string): number | null {
  const normalized = normalizeAmountInput(raw);
  if (normalized === null) return null;
  const tokens = tokenize(normalized);
  if (!tokens || tokens.length === 0 || tokens.length > 32) return null;
  // A bare expression must not end with an operator or start with `*`/`/`.
  const first = tokens[0];
  const last = tokens[tokens.length - 1];
  if (typeof last !== 'number' && last !== ')') return null;
  if (typeof first === 'string' && (first === '*' || first === '/')) return null;
  const value = evaluateTokens(tokens);
  return value === null || !Number.isFinite(value) ? null : value;
}

/** Format a computed amount back into a draft string without float dust. */
export function formatDraftAmount(n: number): string {
  if (!Number.isFinite(n)) return '';
  if (Math.abs(n) < 1e-9) return '0';
  return String(Number(n.toFixed(6)));
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

/** Debit (negative legs) and credit (positive legs) totals, or null if invalid. */
export function splitTotals(splits: SplitDraft[]): { debit: number; credit: number } | null {
  let debit = 0;
  let credit = 0;
  for (const s of splits) {
    const n = parseAmount(s.amount);
    if (n === null) return null;
    if (n < 0) debit += Math.abs(n);
    else credit += n;
  }
  return { debit, credit };
}

/** Amount that would balance the given row, or null when other rows are invalid. */
export function remainderFor(index: number, splits: SplitDraft[]): number | null {
  let sumOthers = 0;
  for (let i = 0; i < splits.length; i += 1) {
    if (i === index) continue;
    const n = parseAmount(splits[i].amount);
    if (n === null) return null;
    sumOthers += n;
  }
  const remainder = -sumOthers;
  return Number.isFinite(remainder) ? remainder : null;
}

export function isBalanced(balance: number | null): boolean {
  return balance !== null && Math.abs(balance) < 1e-6;
}

export function splitErrors(splits: SplitDraft[]): string[] {
  return splits.map((s, i) => {
    if (!s.accountGuid) return `Split ${i + 1}: choose an account`;
    if (parseAmount(s.amount) === null) return `Split ${i + 1}: enter an amount`;
    if (s.showQuantity && s.quantity.trim() && parseAmount(s.quantity) === null)
      return `Split ${i + 1}: enter the share count as a number`;
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
