import { fetchExportAccounts, fetchExportTransactions } from '$lib/tauri';
import type { ExportAccountRow, ExportSplitRow } from '$lib/tauri';
import { buildCsvText, CSV_BOM } from '$lib/utils/csv';

/** Default filename offered in the save dialog for the full-book export. */
export const TRANSACTIONS_EXPORT_FILENAME = 'transactions.csv';
export const ACCOUNTS_EXPORT_FILENAME = 'accounts.csv';

/**
 * One row per split, mirroring GnuCash's multiline export so the file can be
 * re-imported via `File → Import → Import Transactions from CSV`
 * (Multi-split checked). Transaction fields repeat on every split row;
 * unneeded columns (IDs) map to None on import.
 */
export const TRANSACTIONS_EXPORT_HEADERS = [
  'Date',
  'Transaction ID',
  'Number',
  'Description',
  'Notes',
  'Commodity/Currency',
  'Memo',
  'Full Account Name',
  'Account Name',
  'Amount',
  'Reconciled',
  'Price',
  'Split ID'
];

/**
 * Full account tree in parent-before-child order. Balances are omitted on
 * purpose (converted snapshots would corrupt a re-import — balances rebuild
 * from the transactions file); GUIDs round-trip losslessly and GnuCash
 * ignores them.
 */
export const ACCOUNTS_EXPORT_HEADERS = [
  'Type',
  'Full Name',
  'Name',
  'Code',
  'Description',
  'Commodity',
  'Commodity Namespace',
  'Placeholder',
  'GUID',
  'Parent GUID'
];

/** One export row per split with raw native amounts (no FX, no sign flips). */
export function mapExportSplitRow(r: ExportSplitRow): string[] {
  return [
    r.postDate,
    r.txGuid,
    r.num ?? '',
    r.description ?? '',
    r.notes ?? '',
    r.commodity ?? '',
    r.memo ?? '',
    r.fullAccountName,
    r.accountName,
    String(r.amount),
    r.reconcileState ?? '',
    String(r.price),
    r.splitGuid
  ];
}

/** One export row per account, parents before children. */
export function mapExportAccountRow(r: ExportAccountRow): string[] {
  return [
    r.accountType,
    r.fullName,
    r.name,
    r.code ?? '',
    r.description ?? '',
    r.commodity ?? '',
    r.commodityNamespace ?? '',
    r.placeholder ? 'TRUE' : 'FALSE',
    r.guid,
    r.parentGuid ?? ''
  ];
}

/** Full-book transactions CSV text (BOM + newest-first split rows). */
export async function buildTransactionsExport(): Promise<string> {
  const rows = await fetchExportTransactions();
  return CSV_BOM + buildCsvText(TRANSACTIONS_EXPORT_HEADERS, rows.map(mapExportSplitRow));
}

/** Full account tree CSV text (BOM included). */
export async function buildAccountsExport(): Promise<string> {
  const rows = await fetchExportAccounts();
  return CSV_BOM + buildCsvText(ACCOUNTS_EXPORT_HEADERS, rows.map(mapExportAccountRow));
}
