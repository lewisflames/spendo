import { describe, expect, it } from 'vitest';
import { exportDatePattern, exportFilename } from './exporters';

describe('exportDatePattern', () => {
  it('derives day-first slash locales', () => {
    expect(exportDatePattern('en-IE')).toEqual({ parts: 'dmy', separator: '/' });
  });

  it('derives month-first locales', () => {
    expect(exportDatePattern('en-US')).toEqual({ parts: 'mdy', separator: '/' });
  });

  it('derives dot separators', () => {
    expect(exportDatePattern('de-DE')).toEqual({ parts: 'dmy', separator: '.' });
  });

  it('derives year-first locales', () => {
    expect(exportDatePattern('ja-JP')).toEqual({ parts: 'ymd', separator: '/' });
  });

  it('falls back to en-IE for an empty locale', () => {
    expect(exportDatePattern('')).toEqual({ parts: 'dmy', separator: '/' });
  });
});

describe('exportFilename', () => {
  it('slugs the book name and appends the date', () => {
    expect(exportFilename('transactions', 'Every Day.spendo', '2026-09-20')).toBe(
      'Every-Day-transactions-2026-09-20.csv'
    );
  });

  it('falls back to spendo when the name has no usable characters', () => {
    expect(exportFilename('accounts', '  .spendo', '2026-09-20')).toBe(
      'spendo-accounts-2026-09-20.csv'
    );
  });
});
