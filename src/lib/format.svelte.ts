import { appSettings } from '$lib/stores/settings.svelte';

/* eslint-disable svelte/prefer-svelte-reactivity -- Date instances are ephemeral, locale read at call time */

export function formatNumber(n: number): string {
  return n.toLocaleString(appSettings.value.locale);
}

export function formatCurrency(n: number, currency: string): string {
  return new Intl.NumberFormat(appSettings.value.locale, {
    style: 'currency',
    currency
  }).format(n);
}

export function formatDate(date: string | Date, opts?: Intl.DateTimeFormatOptions): string {
  const d = typeof date === 'string' ? new Date(date) : date;
  return d.toLocaleDateString(
    appSettings.value.locale,
    opts ?? { month: 'short', day: 'numeric', year: 'numeric' }
  );
}

export function formatMonth(dateStr: string): string {
  const [y, m] = dateStr.split('-');
  const date = new Date(Number(y), Number(m) - 1);
  return date.toLocaleDateString(appSettings.value.locale, { month: 'short', year: '2-digit' });
}

export function formatCompact(n: number): string {
  if (Math.abs(n) >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (Math.abs(n) >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toFixed(0);
}
