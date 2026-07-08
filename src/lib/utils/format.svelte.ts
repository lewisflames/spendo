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
  if (n === null || n === undefined) return '—';
  const sign = n >= 0 ? '+' : '';
  return `${sign}${n.toFixed(1)}%`;
}

export function balanceColor(balance: number, category: string): string {
  const b = clampZero(balance);
  if (category === 'ASSET') {
    return b >= 0 ? 'text-success' : 'text-destructive';
  }
  if (category === 'EQUITY') {
    return b >= 0 ? 'text-warning' : 'text-destructive';
  }
  if (category === 'LIABILITY') {
    return b >= 0 ? 'text-destructive' : 'text-success';
  }
  return b >= 0 ? 'text-success' : 'text-destructive';
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

export function formatDate(date: string | Date, opts?: Intl.DateTimeFormatOptions): string {
  const d = typeof date === 'string' ? new Date(date) : date;
  const o = opts ?? { month: 'short', day: 'numeric', year: 'numeric' };
  return cachedDateTimeFormat(appSettings.value.locale, o).format(d);
}

export function formatMonth(date: string | Date): string {
  const d = typeof date === 'string' ? new Date(date) : date;
  return cachedDateTimeFormat(appSettings.value.locale, {
    month: 'short',
    year: '2-digit'
  }).format(d);
}

export function formatCompact(n: number): string {
  if (Math.abs(n) >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (Math.abs(n) >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toFixed(0);
}
