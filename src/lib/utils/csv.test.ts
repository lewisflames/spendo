import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { downloadCsv, downloadReportCsv } from './csv';

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

describe('downloadReportCsv', () => {
  it('renders meta preamble, blank line, rows and totals', async () => {
    downloadReportCsv('r.csv', {
      meta: [['Period', '2024-01']],
      headers: ['Account', 'Amount'],
      rows: [['Cash', '100']],
      totals: [['Total', '100']]
    });
    expect(await csvText()).toBe('Period,2024-01\n\nAccount,Amount\nCash,100\n\nTotal,100');
  });

  it('omits preamble and totals when absent', async () => {
    downloadReportCsv('r.csv', { headers: ['A'], rows: [['1'], ['2']] });
    expect(await csvText()).toBe('A\n1\n2');
  });
});
