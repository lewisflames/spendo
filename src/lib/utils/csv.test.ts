import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { buildCsvText, CSV_BOM, downloadCsv } from './csv';

const BOM = '\uFEFF';

let captured: string | null = null;

beforeEach(() => {
  captured = null;
  // Node has Blob but no download plumbing: capture the CSV text handed to
  // the blob and stub the anchor + object-URL calls `saveCsv` needs.
  vi.stubGlobal('URL', {
    createObjectURL: (blob: Blob) => {
      void blob.text().then((text) => {
        captured = text;
      });
      return 'blob:mock';
    },
    revokeObjectURL: () => {}
  });
  vi.stubGlobal('document', {
    createElement: () => ({ click: () => {}, href: '', download: '' })
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

async function csvText(): Promise<string> {
  // createObjectURL captures the text asynchronously via Blob.text().
  for (let i = 0; i < 100 && captured === null; i++) {
    await new Promise((r) => setTimeout(r, 1));
  }
  expect(captured).not.toBeNull();
  // Leading BOM keeps Excel UTF-8 aware; strip it for assertions.
  const text = captured as string;
  return text.startsWith(BOM) ? text.slice(1) : text;
}

describe('downloadCsv', () => {
  it('writes headers plus rows', async () => {
    downloadCsv('a.csv', ['Name', 'Amount'], [['Cash', '100']]);
    expect(await csvText()).toBe('Name,Amount\nCash,100');
  });

  it('quotes cells with commas, quotes and newlines', async () => {
    downloadCsv('a.csv', ['Note'], [['a "quoted", multi\nline']]);
    expect(await csvText()).toBe('Note\n"a ""quoted"", multi\nline"');
  });
});

describe('buildCsvText', () => {
  it('exposes the BOM separately from the body text', () => {
    expect(CSV_BOM).toBe(BOM);
  });

  it('builds header-plus-rows text without touching the DOM', () => {
    expect(buildCsvText(['Name', 'Amount'], [['Cash', '100']])).toBe('Name,Amount\nCash,100');
  });

  it('quotes cells with commas, quotes and newlines', () => {
    expect(buildCsvText(['Note'], [['a "quoted", multi\nline']])).toBe(
      'Note\n"a ""quoted"", multi\nline"'
    );
  });
});
