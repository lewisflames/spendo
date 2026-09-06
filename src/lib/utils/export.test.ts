import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { exportPaginatedCsv, fetchAllRows } from './export';

describe('fetchAllRows', () => {
  it('returns a single page directly', async () => {
    const rows = await fetchAllRows(async () => ({ rows: ['a', 'b'], total: 2 }));
    expect(rows).toEqual(['a', 'b']);
  });

  it('loops pages until the total is reached', async () => {
    const fetchPage = vi.fn(async (page: number) => ({
      rows: [`p${page}`],
      total: 3
    }));
    const rows = await fetchAllRows(fetchPage);
    expect(rows).toEqual(['p1', 'p2', 'p3']);
    expect(fetchPage).toHaveBeenCalledTimes(3);
  });

  it('stops early on an empty page even when the total claims more', async () => {
    const fetchPage = vi.fn(async (page: number) => ({
      rows: page === 1 ? ['a'] : [],
      total: 100
    }));
    const rows = await fetchAllRows(fetchPage);
    expect(rows).toEqual(['a']);
    expect(fetchPage).toHaveBeenCalledTimes(2);
  });
});

describe('exportPaginatedCsv', () => {
  let captured: string | null = null;

  beforeEach(() => {
    captured = null;
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

  it('loops pages and downloads the mapped rows', async () => {
    const fetchPage = vi.fn(async (page: number, pageSize: number) => ({
      rows: page === 1 ? [{ n: 1 }, { n: 2 }] : [],
      total: 2,
      pageSize
    }));
    await exportPaginatedCsv({
      filename: 'out.csv',
      headers: ['N'],
      fetchPage: async (page, size) => {
        const r = await fetchPage(page, size);
        return { rows: r.rows, total: r.total };
      },
      mapRow: (item) => [String(item.n)]
    });
    expect(fetchPage).toHaveBeenCalledWith(1, 500);
    for (let i = 0; i < 100 && captured === null; i++) {
      await new Promise((r) => setTimeout(r, 1));
    }
    expect((captured ?? '').replace(/^\uFEFF/, '')).toBe('N\n1\n2');
  });
});
