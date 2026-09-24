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

const relativeTimeFormatCache = new Map<string, Intl.RelativeTimeFormat>();

function cachedRelativeTimeFormat(locale: string): Intl.RelativeTimeFormat {
  let fmt = relativeTimeFormatCache.get(locale);
  if (!fmt) {
    fmt = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
    relativeTimeFormatCache.set(locale, fmt);
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
  // `changePct` is on a 0–100 scale, so render via number formatting (not
  // `style: 'percent'`) to keep one decimal with locale-aware separators
  // and percent placement (e.g. `12,3 %` in fr-FR).
  const formatted = cachedNumberFormat(appSettings.value.locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1
  }).format(n);
  const sign = n >= 0 ? '+' : '';
  return `${sign}${formatted}%`;
}

/** Snap float dust (e.g. -0.004 from FX math) to a clean zero for display. */
export function clampZero(n: number): number {
  return Math.abs(n) < 0.005 ? 0 : n;
}

export function formatSignedCurrency(n: number, currency: string): string {
  return `${n > 0 ? '+' : ''}${formatCurrency(n, currency)}`;
}

/** Signed variant in the report currency (P&L variances, gain/loss). */
export function formatSignedSessionCurrency(n: number): string {
  return formatSignedCurrency(n, sessionCurrency());
}

/**
 * Split amount for double-entry tables: an explicit '+' marks credits while
 * debits rely on the cell’s sign color (GnuCash-style direction encoding).
 */
export function formatSplitAmount(n: number, currency: string): string {
  if (n === 0) return '0';
  const abs = formatCurrency(Math.abs(n), currency);
  return n > 0 ? `+${abs}` : abs;
}

export function formatNumber(n: number): string {
  return cachedNumberFormat(appSettings.value.locale, {}).format(n);
}

/** Shares/quantities with exactly 2 fraction digits, locale-aware. */
export function formatShares(n: number): string {
  return cachedNumberFormat(appSettings.value.locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  }).format(n);
}

/** Allocation-style percent (1 decimal, no sign prefix). */
export function formatAllocPct(n: number): string {
  const formatted = cachedNumberFormat(appSettings.value.locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1
  }).format(n);
  return `${formatted}%`;
}

export function formatCurrency(n: number, currency: string): string {
  try {
    return cachedNumberFormat(appSettings.value.locale, {
      style: 'currency',
      currency
    }).format(n);
  } catch {
    // A book without a usable ISO code (e.g. the "Unknown" fallback) makes
    // Intl throw; degrade to a plain 2-decimal number instead of crashing.
    return cachedNumberFormat(appSettings.value.locale, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    }).format(n);
  }
}

/** The report currency of the imported book, or EUR before any import. */
export function sessionCurrency(): string {
  const currency = dbSession.value?.summary.currency;
  return currency && /^[A-Za-z]{3}$/.test(currency) ? currency : 'EUR';
}

export function formatSessionCurrency(n: number): string {
  return formatCurrency(n, sessionCurrency());
}

/**
 * Parse date-only strings (`YYYY-MM-DD`) as local dates. The ES spec parses
 * them as UTC midnight, which renders as the previous day west of UTC.
 * Also accepts `YYYY-MM` (first of month, local) for trend charts. Values
 * that start with a date but carry a non-ISO time (`2024-01-15 10:30:00`)
 * are pinned to their date portion as local midnight so parsing never
 * depends on engine-lenient handling; full ISO-8601 `T` timestamps keep
 * their instant semantics.
 */
export function parseLocalDate(date: string | Date): Date {
  if (typeof date === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(date)) {
    return new Date(`${date}T00:00:00`);
  }
  if (typeof date === 'string' && /^\d{4}-\d{2}$/.test(date)) {
    return new Date(`${date}-01T00:00:00`);
  }
  if (
    typeof date === 'string' &&
    /^\d{4}-\d{2}-\d{2}/.test(date) &&
    !/^\d{4}-\d{2}-\d{2}T/.test(date)
  ) {
    return new Date(`${date.slice(0, 10)}T00:00:00`);
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
  if (Number.isNaN(d.getTime())) {
    // Importers can store unparseable source dates verbatim; show them
    // as-is rather than crashing or rendering "Invalid Date".
    return typeof date === 'string' && date !== '' ? date : EMPTY_DASH;
  }
  const o = opts ?? { month: 'short', day: 'numeric', year: 'numeric' };
  return cachedDateTimeFormat(appSettings.value.locale, o).format(d);
}

const RELATIVE_TIME_UNITS: readonly (readonly [Intl.RelativeTimeFormatUnit, number])[] = [
  ['year', 365 * 24 * 60 * 60 * 1000],
  ['month', 30 * 24 * 60 * 60 * 1000],
  ['day', 24 * 60 * 60 * 1000],
  ['hour', 60 * 60 * 1000],
  ['minute', 60 * 1000]
];

/**
 * Elapsed time since a unix-seconds instant, e.g. "2 days ago" (localized,
 * `numeric: 'auto'`). Sub-minute differences collapse to "just now".
 */
export function formatRelativeTime(epochSeconds: number, now = Date.now()): string {
  const diffMs = epochSeconds * 1000 - now;
  for (const [unit, unitMs] of RELATIVE_TIME_UNITS) {
    if (Math.abs(diffMs) >= unitMs) {
      return cachedRelativeTimeFormat(appSettings.value.locale).format(
        Math.round(diffMs / unitMs),
        unit
      );
    }
  }
  return 'just now';
}

/** Integer axis/tick label in the app locale (thousands separators, no decimals). */
export function formatInteger(n: number): string {
  return cachedNumberFormat(appSettings.value.locale, { maximumFractionDigits: 0 }).format(n);
}

/**
 * Human-readable byte size in binary units, e.g. "512 B", "1.5 MB" (one
 * decimal from KB up). Attachment lists only — never used for money.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return EMPTY_DASH;
  if (bytes < 1024) return `${formatInteger(bytes)} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = -1;
  do {
    value /= 1024;
    unit += 1;
  } while (value >= 1024 && unit < units.length - 1);
  const formatted = cachedNumberFormat(appSettings.value.locale, {
    maximumFractionDigits: 1
  }).format(value);
  return `${formatted} ${units[unit]}`;
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
