import { describe, expect, it } from 'vitest';
import { buildCsvText, CSV_BOM } from './csv';

const BOM = '\uFEFF';

describe('buildCsvText', () => {
  it('exposes the BOM separately from the body text', () => {
    expect(CSV_BOM).toBe(BOM);
  });

  it('builds header-plus-rows text', () => {
    expect(buildCsvText(['Name', 'Amount'], [['Cash', '100']])).toBe('Name,Amount\nCash,100');
  });

  it('quotes cells with commas, quotes and newlines', () => {
    expect(buildCsvText(['Note'], [['a "quoted", multi\nline']])).toBe(
      'Note\n"a ""quoted"", multi\nline"'
    );
  });
});
