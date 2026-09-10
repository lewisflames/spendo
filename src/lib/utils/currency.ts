import type { Locale } from '$lib/stores/settings.svelte';

export const BOOK_CURRENCIES = [
  'USD',
  'EUR',
  'GBP',
  'CHF',
  'JPY',
  'CAD',
  'AUD',
  'NZD',
  'SEK',
  'NOK',
  'DKK',
  'PLN',
  'CZK',
  'HUF',
  'BRL',
  'MXN',
  'INR',
  'CNY',
  'KRW',
  'ZAR'
];

const LOCALE_CURRENCY: Record<Locale, string> = {
  'en-US': 'USD',
  'en-GB': 'GBP',
  'en-IE': 'EUR',
  'de-DE': 'EUR',
  'fr-FR': 'EUR',
  'ja-JP': 'JPY',
  'pt-BR': 'BRL',
  'es-ES': 'EUR'
};

export function defaultCurrencyForLocale(locale: Locale): string {
  return LOCALE_CURRENCY[locale] ?? 'USD';
}
