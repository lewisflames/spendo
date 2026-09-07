import { describe, expect, it } from 'vitest';
import { ensureCsvExtension, ensureSpendoExtension } from './book';

describe('ensureSpendoExtension', () => {
  it('keeps paths that already have an extension', () => {
    expect(ensureSpendoExtension('book.spendo')).toBe('book.spendo');
    expect(ensureSpendoExtension('/tmp/backup.db')).toBe('/tmp/backup.db');
  });

  it('appends .spendo otherwise', () => {
    expect(ensureSpendoExtension('book')).toBe('book.spendo');
    expect(ensureSpendoExtension('/tmp/my book')).toBe('/tmp/my book.spendo');
  });
});

describe('ensureCsvExtension', () => {
  it('keeps paths that already have an extension', () => {
    expect(ensureCsvExtension('transactions.csv')).toBe('transactions.csv');
    expect(ensureCsvExtension('/tmp/accounts.txt')).toBe('/tmp/accounts.txt');
  });

  it('appends .csv otherwise', () => {
    expect(ensureCsvExtension('transactions')).toBe('transactions.csv');
    expect(ensureCsvExtension('/tmp/my export')).toBe('/tmp/my export.csv');
  });
});
