import { describe, expect, it } from 'vitest';
import { mockInvoke } from './mock-invoke';

/**
 * The dev fixture backend is only imported by the browser dev branch of
 * `$lib/tauri.ts`, so these tests are the quickest guard against handler
 * drift. Read-only commands are checked first; mutating commands run last
 * because the mock keeps module-level state.
 */
describe('mockInvoke', () => {
  it('rejects unknown commands with a pointer to the mock', async () => {
    await expect(mockInvoke('not_a_command')).rejects.toThrow(/No fixture for 'not_a_command'/);
  });

  it('boots the shell with a balanced book', async () => {
    const home = await mockInvoke<{ hasData: boolean; summary: { totalTransactions: number } }>(
      'fetch_home_data'
    );
    expect(home.hasData).toBe(true);
    expect(home.summary.totalTransactions).toBeGreaterThan(0);

    const accounts = await mockInvoke<{
      trees: unknown[];
      reportCurrency: string;
      balanceCheck: number;
      byType: Record<string, { totalBalance: number }>;
    }>('fetch_accounts');
    expect(accounts.trees).toHaveLength(5);
    expect(accounts.reportCurrency).toBe('USD');
    // Every fixture transaction is balanced, so the identity must hold.
    expect(Math.abs(accounts.balanceCheck)).toBeLessThan(0.005);
    expect(accounts.byType.ASSET.totalBalance).toBeGreaterThan(0);
    expect(accounts.byType.INCOME.totalBalance).toBeGreaterThan(0);
  });

  it('paginates transactions with balanced splits', async () => {
    const page = await mockInvoke<{
      transactions: { guid: string; splits: { amount: number }[]; totalAmount: number }[];
      total: number;
      totalPages: number;
    }>('fetch_transactions', { page: 1, pageSize: 10 });
    expect(page.transactions).toHaveLength(10);
    expect(page.total).toBeGreaterThan(10);
    expect(page.totalPages).toBeGreaterThan(1);
    for (const tx of page.transactions) {
      expect(tx.splits.length).toBeGreaterThanOrEqual(2);
      // The row total is the largest-magnitude split, ties to the lowest
      // amount — mirrors `fetch_splits_for_txns`.
      const amounts = tx.splits.map((split) => split.amount);
      const expected = [...amounts].sort((a, b) => Math.abs(b) - Math.abs(a) || a - b)[0];
      expect(tx.totalAmount).toBeCloseTo(expected, 6);
    }
  });

  it('serves the payloads every page loads', async () => {
    const home = await mockInvoke<{ summary: { currency: string } }>('fetch_home_data');
    expect(home.summary.currency).toBe('USD');

    const detail = await mockInvoke<{ account: { guid: string }; transactions: unknown[] }>(
      'fetch_account_detail',
      { guid: 'checking', page: 1, pageSize: 10 }
    );
    expect(detail.account.guid).toBe('checking');
    expect(detail.transactions.length).toBeGreaterThan(0);

    const txList = await mockInvoke<{ transactions: { guid: string }[] }>('fetch_transactions', {
      page: 1,
      pageSize: 5
    });
    const guid = txList.transactions[0].guid;
    const tx = await mockInvoke<{ transaction: { guid: string; splits: unknown[] } } | null>(
      'fetch_transaction',
      { guid }
    );
    expect(tx?.transaction.guid).toBe(guid);
    expect(tx?.transaction.splits.length).toBeGreaterThanOrEqual(2);

    await expect(mockInvoke('fetch_portfolio_data')).resolves.toMatchObject({
      totalMarketValue: expect.any(Number)
    });
    await expect(
      mockInvoke('fetch_pnl_report', { dateFrom: '2026-01-01', dateTo: '2026-09-20' })
    ).resolves.toMatchObject({ reportCurrency: 'USD', missingFx: [] });
    await expect(mockInvoke('fetch_budget_vs_actual', { year: 2026 })).resolves.toBeInstanceOf(
      Array
    );
    await expect(mockInvoke('fetch_schedules')).resolves.toBeInstanceOf(Array);
    await expect(mockInvoke('fetch_nav_roots')).resolves.toHaveLength(5);
    await expect(
      mockInvoke('fetch_reconcile_info', { accountGuid: 'checking', statementDate: '2026-09-20' })
    ).resolves.toMatchObject({ accountGuid: 'checking' });
    await expect(
      mockInvoke('fetch_holding_detail', { commodityGuid: 'c-vti' })
    ).resolves.toMatchObject({ holding: { mnemonic: 'VTI' } });
    await expect(mockInvoke('search_global', { query: 'rent' })).resolves.toBeTruthy();
  });

  it('round-trips a created transaction', async () => {
    const before = await mockInvoke<{ summary: { totalTransactions: number } }>('fetch_home_data');
    const guid = await mockInvoke<string>('create_transaction', {
      input: {
        postDate: '2026-09-19',
        description: 'Mock smoke test',
        splits: [
          { accountGuid: 'checking', amount: -12.34 },
          { accountGuid: 'groceries', amount: 12.34 }
        ]
      }
    });
    const after = await mockInvoke<{ summary: { totalTransactions: number } }>('fetch_home_data');
    expect(after.summary.totalTransactions).toBe(before.summary.totalTransactions + 1);

    const created = await mockInvoke<{ transaction: { description: string } }>(
      'fetch_transaction',
      {
        guid
      }
    );
    expect(created.transaction.description).toBe('Mock smoke test');

    await mockInvoke('delete_transaction', { guid });
    const reverted = await mockInvoke<{ summary: { totalTransactions: number } }>(
      'fetch_home_data'
    );
    expect(reverted.summary.totalTransactions).toBe(before.summary.totalTransactions);
  });
});
