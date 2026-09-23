import { toast } from '$lib/components/ui/sonner';
import { parseLocalDate, plural } from '$lib/utils/format.svelte';
import type { PricesResult } from '$lib/tauri';

/** A cached price older than this many days is flagged stale in the UI. */
export const STALE_PRICE_DAYS = 7;

/** Whole calendar days since `priceDate` (`YYYY-MM-DD` or full timestamp). */
export function priceStaleDays(priceDate: string): number {
  const then = parseLocalDate(priceDate.slice(0, 10));
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.floor((startOfToday.getTime() - then.getTime()) / 86_400_000);
}

/** Display label of a holding: full name when known, else the ticker mnemonic. */
export function holdingLabel(h: { fullName: string | null; mnemonic: string }): string {
  return h.fullName ?? h.mnemonic;
}

/** Day change in percent, or null without a usable previous price. */
export function dayChangePct(h: { price: number | null; prevPrice: number | null }): number | null {
  if (h.price === null || h.prevPrice === null || h.prevPrice === 0) return null;
  return ((h.price - h.prevPrice) / h.prevPrice) * 100;
}

/** 52-week high/low over the trailing year of price history (or all of it). */
export function range52w(
  history: { date: string; price: number }[] | null | undefined
): { high: number; low: number } | null {
  if (!history || history.length === 0) return null;
  const cutoff = Date.now() - 365 * 24 * 60 * 60 * 1000;
  const window = history.filter((e) => parseLocalDate(e.date).getTime() >= cutoff);
  const source = window.length > 0 ? window : history;
  const prices = source.map((e) => e.price);
  return { high: Math.max(...prices), low: Math.min(...prices) };
}

/**
 * Success/warning toasts for a price refresh; silent when nothing happened.
 * `fixHint` points at the Portfolio symbol overrides (for import flows).
 * Accepts the full `PricesResult` (extra `fetchedAt` ignored).
 */
export function toastPriceResult(
  result: Pick<PricesResult, 'prices' | 'errors'> | null | undefined,
  opts: { fixHint?: boolean } = {}
): void {
  if (!result) return;
  const errorCount = result.errors.length;
  if (errorCount > 0) {
    toast.warning(
      `Prices refreshed with ${plural(errorCount, 'error')}${opts.fixHint ? '. Open Portfolio to fix symbols.' : ''}`
    );
  } else if (result.prices.length > 0) {
    toast.success(opts.fixHint ? 'Fresh prices are in' : 'Prices are up to date');
  }
}
