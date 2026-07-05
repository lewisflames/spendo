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

export function formatRelativeDate(dateStr: string): string {
  const now = Date.now();
  const d = new Date(dateStr).getTime();
  const diffMs = now - d;
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  if (diffDay < 1) {
    if (diffHour < 1) {
      if (diffMin < 1) return 'Just now';
      return `${diffMin}m ago`;
    }
    return `${diffHour}h ago`;
  }
  if (diffDay === 1) return 'Yesterday';
  if (diffDay < 7) return `${diffDay}d ago`;

  const locale = appSettings.value.locale;
  const date = new Date(dateStr);
  return date.toLocaleDateString(locale, { month: 'short', day: 'numeric', year: 'numeric' });
}

export function formatDateGroup(dateStr: string): string {
  const now = Date.now();
  const d = new Date(dateStr).getTime();
  const diffDay = Math.floor((now - d) / 86_400_000);
  if (diffDay < 1) return 'Today';
  if (diffDay === 1) return 'Yesterday';
  if (diffDay < 7) return 'This week';
  return 'Older';
}

export function formatCompact(n: number): string {
  if (Math.abs(n) >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (Math.abs(n) >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toFixed(0);
}
