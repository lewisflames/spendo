import { describe, expect, it } from 'vitest';
import { buildCsvText } from './csv';

describe('buildCsvText', () => {
  it('builds header-plus-rows text', () => {
    expect(buildCsvText(['Name', 'Amount'], [['Cash', '100']])).toBe('Name,Amount\nCash,100');
  });

  it('quotes cells with commas, quotes and newlines', () => {
    expect(buildCsvText(['Note'], [['a "quoted", multi\nline']])).toBe(
      'Note\n"a ""quoted"", multi\nline"'
    );
  });
});
