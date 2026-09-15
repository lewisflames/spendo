import { describe, expect, it } from 'vitest';
import { isSpendoBookPath } from './welcome-utils';

describe('isSpendoBookPath', () => {
  it('accepts the extensions the open dialog allows', () => {
    expect(isSpendoBookPath('/home/you/book.spendo')).toBe(true);
    expect(isSpendoBookPath('C:\\books\\Book.SPENDO')).toBe(true);
    expect(isSpendoBookPath('/tmp/old.db')).toBe(true);
    expect(isSpendoBookPath('/tmp/old.sqlite')).toBe(true);
  });

  it('rejects source books and other files', () => {
    expect(isSpendoBookPath('/tmp/export.gnucash')).toBe(false);
    expect(isSpendoBookPath('/tmp/statement.csv')).toBe(false);
    expect(isSpendoBookPath('/tmp/spendo.txt')).toBe(false);
    expect(isSpendoBookPath('/tmp/notes')).toBe(false);
  });
});
