import { appSettings } from '$lib/stores/settings.svelte';
import { dbSession } from '$lib/stores/db.svelte';

/* eslint-disable svelte/prefer-svelte-reactivity -- Date instances are ephemeral, locale read at call time */

/**
 * Cache for Intl formatters: constructing them is expensive and these run for
 * every table cell on every render. Keys combine everything that affects the
 * formatter; entries live for the session (locale/currency sets are tiny).
 */
const numberFormatCache = new Map<string, Intl.NumberFormat>();

function cachedNumberFormat(locale: string, opts: Intl.NumberFormatOptions): Intl.NumberFormat {
  const key = `${locale}|${JSON.stringify(opts)}`;
  let fmt = numberFormatCache.get(key);
  if (!fmt) {
    fmt = new Intl.NumberFormat(locale, opts);
    numberFormatCache.set(key, fmt);
  }
  return fmt;
}

const dateTimeFormatCache = new Map<string, Intl.DateTimeFormat>();

function cachedDateTimeFormat(
  locale: string,
  opts: Intl.DateTimeFormatOptions
): Intl.DateTimeFormat {
  const key = `${locale}|${JSON.stringify(opts)}`;
  let fmt = dateTimeFormatCache.get(key);
  if (!fmt) {
    fmt = new Intl.DateTimeFormat(locale, opts);
    dateTimeFormatCache.set(key, fmt);
  }
  return fmt;
}

export function signColor(value: number, positiveGood = true): string {
  if (positiveGood) {
    return value >= 0 ? 'text-success' : 'text-destructive';
  }
  return value >= 0 ? 'text-destructive' : 'text-success';
}

export function fmtPct(n: number | null | undefined): string {
  if (n === null || n === undefined) return EMPTY_DASH;
  const sign = n >= 0 ? '+' : '';
  return `${sign}${n.toFixed(1)}%`;
}

/** Snap float dust (e.g. -0.004 from FX math) to a clean zero for display. */
export function clampZero(n: number): number {
  return Math.abs(n) < 0.005 ? 0 : n;
}

export function formatSignedCurrency(n: number, currency: string): string {
  return `${n > 0 ? '+' : ''}${formatCurrency(n, currency)}`;
}

/**
 * Split amount for double-entry tables: an explicit '+' marks credits while
 * debits rely on the cell's sign color (GnuCash-style direction encoding).
 */
export function formatSplitAmount(n: number, currency: string): string {
  if (n === 0) return '0';
  const abs = formatCurrency(Math.abs(n), currency);
  return n > 0 ? `+${abs}` : abs;
}

export function formatNumber(n: number): string {
  return cachedNumberFormat(appSettings.value.locale, {}).format(n);
}

export function formatCurrency(n: number, currency: string): string {
  return cachedNumberFormat(appSettings.value.locale, {
    style: 'currency',
    currency
  }).format(n);
}

/** The report currency of the imported book, or EUR before any import. */
export function sessionCurrency(): string {
  return dbSession.value?.summary.currency ?? 'EUR';
}

export function formatSessionCurrency(n: number): string {
  return formatCurrency(n, sessionCurrency());
}

/**
 * Parse date-only strings (`YYYY-MM-DD`) as local dates. The ES spec parses
 * them as UTC midnight, which renders as the previous day west of UTC.
 * Also accepts `YYYY-MM` (first of month, local) for trend charts.
 */
export function parseLocalDate(date: string | Date): Date {
  if (typeof date === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(date)) {
    return new Date(`${date}T00:00:00`);
  }
  if (typeof date === 'string' && /^\d{4}-\d{2}$/.test(date)) {
    return new Date(`${date}-01T00:00:00`);
  }
  return new Date(date);
}

/** Display placeholder for missing values; keeps table layout stable. */
export const EMPTY_DASH = '—';

/** Calendar year of an ISO date string without UTC-shift risk. */
export function yearOf(isoDate: string): number {
  const year = Number.parseInt(isoDate.slice(0, 4), 10);
  if (Number.isInteger(year)) return year;
  return parseLocalDate(isoDate).getFullYear();
}

export function formatDate(date: string | Date, opts?: Intl.DateTimeFormatOptions): string {
  const d = parseLocalDate(date);
  const o = opts ?? { month: 'short', day: 'numeric', year: 'numeric' };
  return cachedDateTimeFormat(appSettings.value.locale, o).format(d);
}

/** English pluralization for counts, e.g. `plural(3, 'account')` -> "3 accounts". */
export function plural(n: number, word: string): string {
  return `${formatNumber(n)} ${n === 1 ? word : `${word}s`}`;
}

/**
 * Label for a bounded date range, e.g. "Jan 5 – Mar 12, 2024". Em-dash
 * placeholders keep the layout stable when either bound is missing.
 */
export function formatDateRange(from?: string | null, to?: string | null): string {
  const start = from
    ? formatDate(from, { month: 'short', day: 'numeric', year: 'numeric' })
    : EMPTY_DASH;
  const end = to ? formatDate(to, { month: 'short', day: 'numeric', year: 'numeric' }) : EMPTY_DASH;
  return `${start} – ${end}`;
}
