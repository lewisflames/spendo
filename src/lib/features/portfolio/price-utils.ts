import { parseLocalDate } from '$lib/utils/format.svelte';

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
