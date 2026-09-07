import { describe, expect, it, vi } from 'vitest';
import { fetchExportAccounts, fetchExportTransactions } from '$lib/tauri';
import type { ExportAccountRow, ExportSplitRow } from '$lib/tauri';
import {
  ACCOUNTS_EXPORT_HEADERS,
  buildAccountsExport,
  buildTransactionsExport,
  mapExportAccountRow,
  mapExportSplitRow,
  TRANSACTIONS_EXPORT_HEADERS
} from './exporters';

vi.mock('$lib/tauri', () => ({
  fetchExportAccounts: vi.fn(),
  fetchExportTransactions: vi.fn()
}));

const mockedFetchTransactions = vi.mocked(fetchExportTransactions);
const mockedFetchAccounts = vi.mocked(fetchExportAccounts);

function split(
  partial: Partial<ExportSplitRow> & { txGuid: string; splitGuid: string }
): ExportSplitRow {
  return {
    postDate: '2024-01-15',
    description: null,
    num: null,
    notes: null,
    commodity: null,
    memo: null,
    fullAccountName: 'Assets:Checking',
    accountName: 'Checking',
    amount: 0,
    reconcileState: null,
    price: 1,
    ...partial
  };
}

function account(partial: Partial<ExportAccountRow> & { guid: string }): ExportAccountRow {
  return {
    parentGuid: null,
    accountType: 'BANK',
    fullName: 'Assets:Checking',
    name: 'Checking',
    code: null,
    description: null,
    commodity: 'EUR',
    commodityNamespace: 'CURRENCY',
    placeholder: false,
    ...partial
  };
}

describe('mapExportSplitRow', () => {
  it('emits one GnuCash-style row per split with repeated tx fields', () => {
    expect(
      mapExportSplitRow(
        split({
          txGuid: 't1',
          splitGuid: 's1',
          postDate: '2024-03-01',
          description: 'Groceries',
          num: '12',
          notes: 'weekly',
          commodity: 'EUR',
          memo: 'card',
          fullAccountName: 'Assets:Checking',
          accountName: 'Checking',
          amount: -50,
          reconcileState: 'c',
          price: 1
        })
      )
    ).toEqual([
      '2024-03-01',
      't1',
      '12',
      'Groceries',
      'weekly',
      'EUR',
      'card',
      'Assets:Checking',
      'Checking',
      '-50',
      'c',
      '1',
      's1'
    ]);
  });

  it('blanks nullish fields', () => {
    expect(mapExportSplitRow(split({ txGuid: 't', splitGuid: 's' }))).toEqual([
      '2024-01-15',
      't',
      '',
      '',
      '',
      '',
      '',
      'Assets:Checking',
      'Checking',
      '0',
      '',
      '1',
      's'
    ]);
  });
});

describe('mapExportAccountRow', () => {
  it('emits the full-fidelity tree row', () => {
    expect(
      mapExportAccountRow(
        account({
          guid: 'checking',
          parentGuid: 'assets',
          accountType: 'BANK',
          fullName: 'Assets:Checking',
          name: 'Checking',
          code: '1001',
          description: 'Everyday',
          placeholder: false
        })
      )
    ).toEqual([
      'BANK',
      'Assets:Checking',
      'Checking',
      '1001',
      'Everyday',
      'EUR',
      'CURRENCY',
      'FALSE',
      'checking',
      'assets'
    ]);
  });

  it('marks placeholders and blanks missing parents', () => {
    const row = mapExportAccountRow(account({ guid: 'a', placeholder: true }));
    expect(row[7]).toBe('TRUE');
    expect(row[9]).toBe('');
  });
});

describe('buildTransactionsExport', () => {
  it('dumps one row per split with BOM + headers', async () => {
    mockedFetchTransactions.mockResolvedValue([
      split({ txGuid: 't1', splitGuid: 's1', description: 'one' }),
      split({ txGuid: 't1', splitGuid: 's2', description: 'one' })
    ]);
    const text = await buildTransactionsExport();
    expect(text.startsWith('\uFEFF')).toBe(true);
    const lines = text.slice(1).split('\n');
    expect(lines[0]).toBe(TRANSACTIONS_EXPORT_HEADERS.join(','));
    expect(lines).toHaveLength(3);
    expect(lines[1]).toContain('s1');
    expect(lines[2]).toContain('s2');
  });
});

describe('buildAccountsExport', () => {
  it('exports the full tree rows with BOM + headers', async () => {
    mockedFetchAccounts.mockResolvedValue([
      account({ guid: 'assets', fullName: 'Assets', name: 'Assets', placeholder: true }),
      account({ guid: 'checking', parentGuid: 'assets' })
    ]);
    const text = await buildAccountsExport();
    expect(text.startsWith('\uFEFF')).toBe(true);
    const lines = text.slice(1).split('\n');
    expect(lines[0]).toBe(ACCOUNTS_EXPORT_HEADERS.join(','));
    expect(lines).toHaveLength(3);
    expect(lines[1]).toContain('Assets');
    expect(lines[2]).toContain('Assets:Checking');
  });
});
