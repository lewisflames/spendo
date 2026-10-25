import { describe, expect, it } from 'vitest';
import {
  isSpendoBookPath,
  MAX_RECENT_BOOKS,
  parseRecentBooks,
  withRecentBook
} from './welcome-utils';

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

describe('parseRecentBooks', () => {
  it('drops malformed entries and duplicates, newest first', () => {
    expect(
      parseRecentBooks([
        { path: '/a.spendo', openedAt: 10 },
        { path: '/b.spendo', openedAt: 30 },
        { path: '/a.spendo', openedAt: 5 },
        { path: '', openedAt: 1 },
        { path: '/c.spendo', openedAt: 'yesterday' },
        null,
        'x'
      ])
    ).toEqual([
      { path: '/b.spendo', openedAt: 30 },
      { path: '/a.spendo', openedAt: 10 }
    ]);
  });

  it('treats anything but an array as empty', () => {
    expect(parseRecentBooks({ path: '/a.spendo' })).toEqual([]);
    expect(parseRecentBooks(null)).toEqual([]);
  });
});

describe('withRecentBook', () => {
  it('moves a reopened book to the front with the new time', () => {
    const books = [
      { path: '/a.spendo', openedAt: 2 },
      { path: '/b.spendo', openedAt: 1 }
    ];
    expect(withRecentBook(books, '/b.spendo', 9)).toEqual([
      { path: '/b.spendo', openedAt: 9 },
      { path: '/a.spendo', openedAt: 2 }
    ]);
  });

  it('caps the list', () => {
    const books = Array.from({ length: MAX_RECENT_BOOKS }, (_, i) => ({
      path: `/${i}.spendo`,
      openedAt: 100 - i
    }));
    const next = withRecentBook(books, '/new.spendo', 200);
    expect(next).toHaveLength(MAX_RECENT_BOOKS);
    expect(next[0].path).toBe('/new.spendo');
    expect(next.at(-1)?.path).toBe(`/${MAX_RECENT_BOOKS - 2}.spendo`);
  });
});
