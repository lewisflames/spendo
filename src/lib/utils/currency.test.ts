import { describe, expect, it } from 'vitest';
import { BOOK_CURRENCIES, defaultCurrencyForLocale } from './currency';

describe('defaultCurrencyForLocale', () => {
  it('maps known locales to their book currency', () => {
    expect(defaultCurrencyForLocale('en-US')).toBe('USD');
    expect(defaultCurrencyForLocale('en-IE')).toBe('EUR');
    expect(defaultCurrencyForLocale('ja-JP')).toBe('JPY');
    expect(defaultCurrencyForLocale('pt-BR')).toBe('BRL');
  });
});

describe('BOOK_CURRENCIES', () => {
  it('covers the locale defaults', () => {
    for (const locale of ['en-US', 'en-IE', 'ja-JP', 'pt-BR'] as const) {
      expect(BOOK_CURRENCIES).toContain(defaultCurrencyForLocale(locale));
    }
  });
});
