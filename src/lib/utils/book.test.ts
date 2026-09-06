import { describe, expect, it } from 'vitest';
import { ensureSpendoExtension } from './book';

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
