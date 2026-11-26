import { commands as generated } from './bindings';
import type {
  BulkAction,
  BulkTarget,
  Filters,
  SortDir,
  SplitStateUpdate,
  TaxRoleAssignment,
  TxSortKey
} from './bindings';
import type { Units } from './utils/units';

/*
 * Typed access to the Rust commands. Payload and input types are generated
 * from the Rust structs into `bindings.ts` (`bun run bindings`; `cargo test`
 * fails while it is stale), so a renamed field breaks `bun run check` instead
 * of failing at runtime. This module only adds the `bookChanged` hook and a
 * few wrappers whose signature reads better than the raw command's.
 */
export type * from './bindings';

let bookChangedHandler: (() => void) | null = null;

/** Called whenever a command fails with `bookChanged` (the book file was
 * replaced behind the open connection), so the book can be reloaded. */
export function onBookChanged(handler: () => void): void {
  bookChangedHandler = handler;
}

type Commands = typeof generated;

function notifyBookChanged<F extends (...args: never[]) => Promise<unknown>>(command: F): F {
  return ((...args: Parameters<F>) =>
    command(...args).catch((e: unknown) => {
      if ((e as { kind?: unknown } | null)?.kind === 'bookChanged') bookChangedHandler?.();
      throw e;
    })) as F;
}

const api = Object.fromEntries(
  Object.entries(generated).map(([name, command]) => [name, notifyBookChanged(command)])
) as Commands;

/** `fetchHomeData` → `fetch_home_data`, the name the command is invoked by. */
type SnakeCase<S extends string> = S extends `${infer Head}${infer Tail}`
  ? `${Head extends Lowercase<Head> ? Head : `_${Lowercase<Head>}`}${SnakeCase<Tail>}`
  : S;

/** Resolved value of every command, keyed by its invoke name (IPC mocks use it). */
export type CommandResults = {
  [K in keyof Commands as SnakeCase<K & string>]: Awaited<ReturnType<Commands[K]>>;
};

export type Command = keyof CommandResults;

export const {
  fetchHomeData,
  fetchNetWorthTrend,
  fetchActiveBook,
  bookFilesExist,
  openBook,
  createBookAt,
  saveBookAs,
  closeBook,
  reloadChangedBook,
  listSyncConflicts,
  reviewSyncConflict,
  fetchBackups,
  createBackup,
  deleteBackup,
  restoreBackup,
  setBaseCurrency,
  clearAppDb,
  getBookSettings,
  setBookSetting,
  csvDetect,
  csvPreview,
  csvCommit,
  deleteCsvPreset,
  fetchImportRules,
  createImportRule,
  updateImportRule,
  deleteImportRule,
  fetchAccounts,
  fetchBalanceSheet,
  fetchBookCurrencies,
  fetchNavBadges,
  createAccount,
  updateAccount,
  deleteAccount,
  fetchTransaction,
  createTransaction,
  updateTransaction,
  fetchTags,
  deleteTag,
  fetchPayees,
  deletePayee,
  fetchTaxSummary,
  fetchTaxRoles,
  exportTaxSummaryCsv,
  fetchAttachments,
  addAttachments,
  deleteAttachment,
  exportAttachment,
  openAttachment,
  fetchReconcileInfo,
  reopenLastReconciliation,
  fetchReconcileSummaries,
  exportTransactionsCsv,
  exportAccountsCsv,
  refreshPrices,
  fetchAccountBalanceTrend,
  fetchPnlTrend,
  fetchSpendingReport,
  fetchSymbolOverrides,
  setSymbolOverride,
  deleteSymbolOverride,
  setBudgetYearAmount,
  fetchBudgetYears,
  deleteBudgetYear,
  fetchBudgetVsActual,
  fetchBudgetDetail,
  fetchBudgetDateRange,
  createSchedule,
  updateSchedule,
  deleteSchedule,
  setScheduleActive,
  fetchSchedules,
  fetchSchedule,
  postScheduleOccurrence,
  skipScheduleOccurrence,
  postDueSchedules,
  fetchTransitCheck,
  fetchPortfolioData,
  fetchAllocationTargets,
  saveAllocationTargets,
  setDividendLinks,
  fetchRealizedGains,
  exportRealizedGainsCsv
} = api;

export const fetchTransactions = (
  page: number,
  pageSize: number,
  filters?: Filters,
  sortBy?: TxSortKey,
  sortDir?: SortDir
) => api.fetchTransactions(page, pageSize, filters ?? null, sortBy ?? null, sortDir ?? null);

export const fetchTransactionHeatmap = (filters?: Filters) =>
  api.fetchTransactionHeatmap(filters ?? null);

export const deleteTransaction = (guid: string, unreconcile = false) =>
  api.deleteTransaction(guid, unreconcile);

/** Takes a safety backup first, then applies the action in one transaction. */
export const bulkEditTransactions = (target: BulkTarget, action: BulkAction) =>
  api.bulkEditTransactions({ target, action });

/** Renaming onto another tag's name merges them; resolves to the surviving tag's GUID. */
export const renameTag = (guid: string, name: string) => api.renameTag({ guid, name });

/** Renaming onto another payee's name merges them; resolves to the surviving payee's GUID. */
export const renamePayee = (guid: string, name: string) => api.renamePayee({ guid, name });

/** Replaces every assignment. */
export const saveTaxRoles = (roles: TaxRoleAssignment[]) => api.saveTaxRoles({ roles });

/** Move a rule one slot up (-1) or down (1); returns the reordered list. */
export const moveImportRule = (id: number, direction: -1 | 1) => api.moveImportRule(id, direction);

export const markReconcileSplits = (
  accountGuid: string,
  statementDate: string,
  updates: SplitStateUpdate[]
) => api.markReconcileSplits({ accountGuid, statementDate, updates });

export const finishReconciliation = (
  accountGuid: string,
  statementDate: string,
  endingBalance: Units,
  updates: SplitStateUpdate[]
) => api.finishReconciliation({ accountGuid, statementDate, endingBalance, updates });

export const fetchAccountDetail = (
  guid: string,
  page: number,
  pageSize: number,
  dateFrom?: string,
  dateTo?: string,
  search?: string,
  sortBy?: TxSortKey,
  sortDir?: SortDir
) =>
  api.fetchAccountDetail(
    guid,
    page,
    pageSize,
    dateFrom ?? null,
    dateTo ?? null,
    search ?? null,
    sortBy ?? null,
    sortDir ?? null
  );

export const fetchPnlReport = (
  dateFrom: string,
  dateTo: string,
  prevDateFrom?: string,
  prevDateTo?: string
) => api.fetchPnlReport(dateFrom, dateTo, prevDateFrom ?? null, prevDateTo ?? null);

export const fetchHoldingDetail = (
  commodityGuid: string,
  page: number,
  pageSize: number,
  search?: string
) => api.fetchHoldingDetail(commodityGuid, page, pageSize, search ?? null);

export const searchGlobal = (query: string, limitPerGroup?: number) =>
  api.searchGlobal(query, limitPerGroup ?? null);
