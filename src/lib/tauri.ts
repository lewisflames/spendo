import { invoke as tauriInvoke, isTauri } from '@tauri-apps/api/core';

/**
 * Dev-only fixture backend: when `bun run dev` is opened in a plain browser
 * (no Tauri runtime) calls resolve against `$lib/dev/mock-invoke.ts` so
 * frontend/UI work doesn’t need the desktop shell. `import.meta.env.DEV` is a
 * build-time constant, so the mock and its fixtures are dropped from production
 * bundles.
 */
const invoke = <K extends Command>(
  cmd: K,
  args?: Record<string, unknown>
): Promise<CommandPayloads[K]> => {
  if (import.meta.env.DEV && !isTauri()) {
    return import('./dev/mock-invoke').then((mock) => mock.mockInvoke(cmd, args));
  }
  return tauriInvoke<CommandPayloads[K]>(cmd, args);
};

interface HomeDataResult {
  hasData: boolean;
  summary: DataSummary | null;
}

export interface DataSummary {
  currency: string;
  totalAccounts: number;
  totalTransactions: number;
  earliestDate: string | null;
  latestDate: string | null;
  netWorth: number;
}

/** Sortable ledger columns shared by transactions + account detail. Backend falls back silently on unknown values, so keep this in sync. */
export type TxSortKey = 'date' | 'description' | 'amount';
type SortDir = 'asc' | 'desc';

interface ImportResult {
  accountsAdded: number;
  transactionsAdded: number;
  splitsAdded: number;
  commoditiesAdded: number;
  accountsUpdated: number;
  transactionsUpdated: number;
  splitsUpdated: number;
  accountsOrphaned: number;
  transactionsOrphaned: number;
  splitsOrphaned: number;
  commoditiesOrphaned: number;
  schedulesAdded: number;
  schedulesUpdated: number;
  schedulesSkipped: number;
  totalAccounts: number;
  totalTransactions: number;
  summary: DataSummary;
  prices: PricesResult;
}

export interface AccountNode {
  guid: string;
  name: string;
  accountType: string;
  code: string | null;
  description: string | null;
  placeholder: boolean;
  commodity: string | null;
  commodityGuid: string | null;
  balance: number;
  txCount: number;
  children: AccountNode[];
}

interface TypeSummary {
  count: number;
  totalBalance: number;
}

export interface AccountsResult {
  trees: AccountNode[];
  byType: Record<string, TypeSummary>;
  totalAccounts: number;
  reportCurrency: string;
  /** Cumulative income minus expenses to date. */
  retainedEarnings: number;
  /** Market minus cost for priced investments. */
  unrealizedGains: number;
  /** Assets − (liabilities + equity + unrealized + retained); ~0 when balanced. */
  balanceCheck: number;
  missingFx: string[];
  missingPrices: MissingPrice[];
}

interface SplitInfo {
  guid: string;
  accountGuid: string;
  accountName: string;
  amount: number;
  memo?: string | null;
}

export interface TransactionRow {
  guid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitInfo[];
  totalAmount: number;
}

export interface Filters {
  search?: string;
  dateFrom?: string;
  dateTo?: string;
}

interface TransactionsResult {
  transactions: TransactionRow[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

export interface HeatmapPoint {
  /** YYYY-MM-DD */
  date: string;
  count: number;
}

export interface SplitDetail extends SplitInfo {
  /** Raw signed quantity; share counts for investment accounts, else equals amount. */
  quantity: number;
  commodity?: string | null;
  reconciled: boolean;
  /** GnuCash reconcile state: 'n' new, 'c' cleared, 'y' reconciled. */
  reconcileState?: string | null;
  accountDescription?: string | null;
  accountCode?: string | null;
}

export interface TransactionDetail {
  guid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitDetail[];
  totalAmount: number;
  /** Attached receipts/documents; deleting the transaction deletes them for good. */
  attachmentCount: number;
}

interface TransactionDetailResult {
  transaction: TransactionDetail;
  prevGuid?: string | null;
  nextGuid?: string | null;
}

export interface AccountBalancePoint {
  /** YYYY-MM-DD */
  date: string;
  balance: number;
}

/**
 * Response of `refresh_prices` (also nested as `ImportResult.prices` after an
 * automatic post-import fetch). The UI only consumes per-commodity errors.
 */
export interface PricesResult {
  prices: { mnemonic: string; price: number; priceDate: string; currency: string }[];
  fetchedAt: string;
  errors: { mnemonic: string; error: string }[];
}

export interface PriceHistoryEntry {
  date: string;
  price: number;
  currency: string;
}

export interface DetailTransaction {
  guid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitInfo[];
  /** Other accounts involved, outside the viewed account’s subtree. */
  counterAccounts: string[];
  /** Signed effect on the viewed subtree, in report currency at the tx date. */
  amount: number;
  runningBalance: number;
  /** GnuCash reconcile state of the viewed subtree’s splits: 'n'/'c'/'y'. */
  reconcileState?: string | null;
}

/** Direct child of the viewed account, with tree-consistent balances. */
export interface ChildAccountInfo {
  guid: string;
  name: string;
  description: string | null;
  code: string | null;
  commodity: string | null;
  commodityGuid: string | null;
  accountType: string;
  balance: number;
  txCount: number;
}

export interface AccountDetailInfo {
  guid: string;
  name: string;
  accountType: string;
  code: string | null;
  description: string | null;
  placeholder: boolean;
  commodity: string | null;
  commodityGuid: string | null;
  parentGuid: string | null;
  balance: number;
  /** Whole subtree minus self — not direct children. */
  descendantsCount: number;
  childAccounts: ChildAccountInfo[];
}

/** The viewed account’s parent; `null` for top-level accounts. */
export interface ParentAccountInfo {
  guid: string;
  name: string;
  description: string | null;
  balance: number;
}

export interface AccountSummary {
  totalTransactions: number;
  totalIncome: number;
  totalExpenses: number;
}

export interface AccountDetailResult {
  account: AccountDetailInfo;
  summary: AccountSummary;
  transactions: DetailTransaction[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
  /** The account node with its full subtree (tree-consistent balances). */
  subtree: AccountNode | null;
  /** The viewed account’s parent, for the pie’s parent-ratio label. */
  parent: ParentAccountInfo | null;
}

export const fetchHomeData = (): Promise<HomeDataResult> => invoke('fetch_home_data');

/** One ACCOUNTS sidebar destination: the top-level account for a category. */
export interface NavRoot {
  category: string;
  guid: string | null;
  name: string | null;
}

export const fetchNavRoots = (): Promise<NavRoot[]> => invoke('fetch_nav_roots');

export interface NavBadges {
  dueSchedules: number;
  unclearedTransactions: number;
}

export const fetchNavBadges = (): Promise<NavBadges> => invoke('fetch_nav_badges');

export const clearAppDb = (): Promise<void> => invoke('clear_app_db');

// ── Books (document model: one active `.spendo` file opened in place) ──

/** Active book path, or `null` while book-less (fresh install). */
interface ActiveBook {
  path: string | null;
}

export const fetchActiveBook = (): Promise<ActiveBook> => invoke('fetch_active_book');

interface EmptyBookResult {
  hasData: boolean;
  summary: DataSummary;
}

/** Closing returns no summary: the session is book-less afterwards. */
interface CloseBookResult {
  hasData: boolean;
  summary: DataSummary | null;
}

export const openBook = (path: string): Promise<EmptyBookResult> => invoke('open_book', { path });

export const createBookAt = (path: string, currency: string): Promise<EmptyBookResult> =>
  invoke('create_book_at', { path, currency });

export const saveBookAs = (path: string): Promise<EmptyBookResult> =>
  invoke('save_book_as', { path });

/** Close the active book and return to the book-less placeholder. */
export const closeBook = (): Promise<CloseBookResult> => invoke('close_book');

// ── Book-scoped settings (travel with the `.spendo` file) ──

export type PnlPeriodId = 'this_month' | 'this_quarter' | 'this_year' | 'last_12_months';
type PriceAutoRefresh = 'off' | 'on_startup';
export type CsvDelimiter = ',' | ';';
export type ExportDateFormat = 'iso' | 'locale';

export interface BookSettings {
  fiscalYearStartMonth: number;
  defaultPnlPeriod: PnlPeriodId;
  priceAutoRefresh: PriceAutoRefresh;
  exportCsvDelimiter: CsvDelimiter;
  exportDateFormat: ExportDateFormat;
}

export const getBookSettings = (): Promise<BookSettings> => invoke('get_book_settings');

export const setBookSetting = (key: string, value: string): Promise<BookSettings> =>
  invoke('set_book_setting', { key, value });

export interface GnuCashPreview {
  path: string;
  filename: string;
  sha256: string;
  accountsAdded: number;
  accountsUpdated: number;
  accountsOrphaned: number;
  transactionsAdded: number;
  transactionsUpdated: number;
  transactionsOrphaned: number;
  splitsAdded: number;
  splitsUpdated: number;
  splitsOrphaned: number;
  commoditiesAdded: number;
  commoditiesUpdated: number;
  commoditiesOrphaned: number;
  /** Scheduled transactions merged by GUID (never swept). */
  schedulesAdded: number;
  schedulesUpdated: number;
  orphanAccountNames: string[];
  /** Source schedules Spendo can’t represent, with readable reasons. */
  skippedSchedules: { name: string; reason: string }[];
}

interface GnuCashImportInput {
  path: string;
  filename: string;
  /** Hash returned by `previewGnucashImport`; the merge refuses stale files. */
  sha256: string;
  /** When false, rows absent from the source survive (no orphan sweep). */
  deleteOrphans: boolean;
}

export const previewGnucashImport = (path: string, filename: string): Promise<GnuCashPreview> =>
  invoke('preview_gnucash_import', { path, filename });

export const importGnucash = (input: GnuCashImportInput): Promise<ImportResult> =>
  invoke('import_gnucash', { input });

// ── CSV import (bank-statement wizard: detect → map → preview → commit) ──

export interface CsvMapping {
  dateCol: number;
  descCol: number;
  amountCol?: number | null;
  debitCol?: number | null;
  creditCol?: number | null;
  numCol?: number | null;
  notesCol?: number | null;
}

interface CsvPreviewInput {
  path: string;
  /** SHA-256 from `csvDetect`; preview/commit reject changed files. */
  fileSha256: string;
  encoding?: string | null;
  delimiter?: string | null;
  hasHeader?: boolean | null;
  mapping: CsvMapping;
  targetAccountGuid: string;
  offsetAccountGuid: string;
  datePreference?: string | null;
}

/** Saved mapping for one bank-statement header layout. */
export interface CsvPreset {
  signature: string;
  name: string;
  encoding: string | null;
  delimiter: string;
  hasHeader: boolean;
  datePreference: string;
  mapping: CsvMapping;
  targetAccountGuid: string;
  offsetAccountGuid: string;
}

export interface CsvDetectResult {
  encoding: string;
  delimiter: string;
  hasHeader: boolean;
  headers: string[];
  columnCount: number;
  rowCount: number;
  sampleRows: string[][];
  filename: string;
  sha256: string;
  /** Header-row signature (`null` for headerless files). */
  signature: string | null;
  /** Saved mapping for this layout, when the book has one. */
  preset: CsvPreset | null;
}

interface CsvPreviewRow {
  key: number;
  raw: string[];
  postDate: string | null;
  description: string | null;
  amount: number | null;
  valid: boolean;
  duplicate: boolean;
  /** Category the row will post against (rule match or fallback account). */
  offsetAccountName: string;
  /** True when an import rule picked the category. */
  ruleMatched: boolean;
  error: string | null;
}

export interface CsvPreviewResult {
  rows: CsvPreviewRow[];
  totalRows: number;
  validCount: number;
  duplicateCount: number;
  errorCount: number;
  minDate: string | null;
  maxDate: string | null;
}

export interface CsvCommitResult {
  created: number;
  skippedDuplicates: number;
  skippedErrors: number;
  label: string;
}

export const csvDetect = (path: string): Promise<CsvDetectResult> => invoke('csv_detect', { path });

export const csvPreview = (input: CsvPreviewInput): Promise<CsvPreviewResult> =>
  invoke('csv_preview', { input });

export const csvCommit = (
  input: CsvPreviewInput & { selectedKeys: number[]; rememberMapping: boolean }
): Promise<CsvCommitResult> => invoke('csv_commit', { input });

export const deleteCsvPreset = (signature: string): Promise<void> =>
  invoke('delete_csv_preset', { signature });

// ── Import rules (CSV row description → category) ──

export interface ImportRule {
  id: number;
  priority: number;
  matchType: 'contains' | 'equals';
  pattern: string;
  accountGuid: string;
  accountName: string;
  accountDescription: string | null;
  accountCode: string | null;
}

export interface ImportRuleInput {
  priority: number;
  matchType: 'contains' | 'equals';
  pattern: string;
  accountGuid: string;
}

export const fetchImportRules = (): Promise<ImportRule[]> => invoke('fetch_import_rules');

export const createImportRule = (input: ImportRuleInput): Promise<ImportRule> =>
  invoke('create_import_rule', { input });

export const updateImportRule = (id: number, input: ImportRuleInput): Promise<ImportRule> =>
  invoke('update_import_rule', { id, input });

export const deleteImportRule = (id: number): Promise<void> => invoke('delete_import_rule', { id });

/** Move a rule one slot up (-1) or down (1); returns the reordered list. */
export const moveImportRule = (id: number, direction: -1 | 1): Promise<ImportRule[]> =>
  invoke('move_import_rule', { id, direction });

export const setBaseCurrency = (currency: string): Promise<EmptyBookResult> =>
  invoke('set_base_currency', { currency });

export const fetchAccounts = (): Promise<AccountsResult> => invoke('fetch_accounts');

export interface BookCurrency {
  guid: string;
  mnemonic: string;
}

export const fetchBookCurrencies = (): Promise<BookCurrency[]> => invoke('fetch_book_currencies');

export interface AccountInput {
  name: string;
  accountType: string;
  parentGuid?: string | null;
  code?: string | null;
  description?: string | null;
  commodityGuid?: string | null;
  placeholder: boolean;
}

export const createAccount = (input: AccountInput): Promise<string> =>
  invoke('create_account', { input });

export const updateAccount = (guid: string, input: AccountInput): Promise<void> =>
  invoke('update_account', { guid, input });

export const deleteAccount = (guid: string): Promise<void> => invoke('delete_account', { guid });

export interface SplitInput {
  accountGuid: string;
  amount: number;
  quantity?: number | null;
  memo?: string | null;
}

export interface TxInput {
  postDate: string;
  description?: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitInput[];
  /** Edit form only: the user confirmed putting cleared/reconciled splits back to New. */
  unreconcile?: boolean;
}

export const createTransaction = (input: TxInput): Promise<string> =>
  invoke('create_transaction', { input });

export const updateTransaction = (guid: string, input: TxInput): Promise<void> =>
  invoke('update_transaction', { guid, input });

export const deleteTransaction = (guid: string, unreconcile = false): Promise<void> =>
  invoke('delete_transaction', { guid, unreconcile });

export const fetchTransactions = (
  page: number,
  pageSize: number,
  filters?: Filters,
  sortBy?: TxSortKey,
  sortDir?: SortDir
): Promise<TransactionsResult> =>
  invoke('fetch_transactions', {
    page,
    pageSize,
    filters: filters ?? null,
    sortBy: sortBy ?? null,
    sortDir: sortDir ?? null
  });

export const fetchTransactionHeatmap = (filters?: Filters): Promise<HeatmapPoint[]> =>
  invoke('fetch_transaction_heatmap', { filters: filters ?? null });

/** `null` when no transaction with that GUID exists. */
export const fetchTransaction = (guid: string): Promise<TransactionDetailResult | null> =>
  invoke('fetch_transaction', { guid });

// ── Attachments (receipts/documents stored inside the book) ──

export interface AttachmentInfo {
  guid: string;
  filename: string;
  mimeType: string;
  byteSize: number;
  createdAt: string;
}

interface AttachmentAddInput {
  txGuid: string;
  paths: string[];
}

export const fetchAttachments = (txGuid: string): Promise<AttachmentInfo[]> =>
  invoke('fetch_attachments', { txGuid });

/** One bad file fails the whole batch with its own message. */
export const addAttachments = (input: AttachmentAddInput): Promise<AttachmentInfo[]> =>
  invoke('add_attachments', { input });

export const deleteAttachment = (guid: string): Promise<void> =>
  invoke('delete_attachment', { guid });

export const exportAttachment = (guid: string, destPath: string): Promise<void> =>
  invoke('export_attachment', { guid, destPath });

/** Writes to the app cache and opens the OS default app for the type. */
export const openAttachment = (guid: string): Promise<void> => invoke('open_attachment', { guid });

// ── Recurring schedules (manual remind + post; in-app reminders only) ──

export type ScheduleFrequency =
  'daily' | 'weekly' | 'biweekly' | 'monthly' | 'quarterly' | 'yearly';

interface ScheduleSplit {
  accountGuid: string;
  accountName?: string | null;
  amount: number;
  quantity?: number | null;
  memo?: string | null;
}

export interface Schedule {
  guid: string;
  name: string;
  frequency: ScheduleFrequency;
  intervalMult: number;
  /** YYYY-MM-DD of the first occurrence. */
  startDate: string;
  /** YYYY-MM-DD, or null when indefinite. */
  endDate: string | null;
  maxOccurrences: number | null;
  occurrencesPosted: number;
  /** Occurrences skipped; counts toward `maxOccurrences` like posted ones. */
  skippedCount: number;
  /** YYYY-MM-DD, or null when completed. */
  nextDueDate: string | null;
  active: boolean;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  splits: ScheduleSplit[];
  /** Sum of the positive template split amounts (a raw sum is always 0). */
  previewAmount: number;
  createdAt: string;
}

export interface ScheduleInput {
  name: string;
  frequency: ScheduleFrequency;
  intervalMult?: number | null;
  startDate: string;
  endDate?: string | null;
  maxOccurrences?: number | null;
  description?: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitInput[];
}

export const fetchSchedules = (): Promise<Schedule[]> => invoke('fetch_schedules');

export const fetchSchedule = (guid: string): Promise<Schedule> =>
  invoke('fetch_schedule', { guid });

export const createSchedule = (input: ScheduleInput): Promise<Schedule> =>
  invoke('create_schedule', { input });

export const updateSchedule = (guid: string, input: ScheduleInput): Promise<Schedule> =>
  invoke('update_schedule', { guid, input });

export const deleteSchedule = (guid: string): Promise<void> => invoke('delete_schedule', { guid });

export const setScheduleActive = (guid: string, active: boolean): Promise<Schedule> =>
  invoke('set_schedule_active', { guid, active });

/** Posts the due occurrence; resolves with the new transaction GUID. */
export const postScheduleOccurrence = (guid: string): Promise<string> =>
  invoke('post_schedule_occurrence', { guid });

/** Skips the due occurrence; resolves with the new pointer (`null` = completed). */
export const skipScheduleOccurrence = (guid: string): Promise<string | null> =>
  invoke('skip_schedule_occurrence', { guid });

/** Full-book transactions CSV written straight to the chosen path. */
interface TransactionsExportInput {
  path: string;
  delimiter: ',' | ';';
  /** `iso` keeps `YYYY-MM-DD`; `locale` uses the probe-derived parts. */
  dateFormat: 'iso' | 'locale';
  dateParts?: 'dmy' | 'mdy' | 'ymd' | null;
  dateSeparator?: string | null;
}

/** Full account-tree CSV written straight to the chosen path. */
interface AccountsExportInput {
  path: string;
  delimiter: ',' | ';';
}

export const exportTransactionsCsv = (input: TransactionsExportInput): Promise<void> =>
  invoke('export_transactions_csv', { input });

export const exportAccountsCsv = (input: AccountsExportInput): Promise<void> =>
  invoke('export_accounts_csv', { input });

/** Full-book GnuCash SQLite export written straight to the chosen path. */
export interface GnuCashExportResult {
  accounts: number;
  transactions: number;
  splits: number;
  prices: number;
  budgets: number;
  schedules: number;
  futureDated: number;
  attachmentsSkipped: number;
  /** Splits with more decimals than GnuCash’s smallest unit (it rounds them). */
  roundedAmounts: number;
}

export const exportGnuCash = (path: string): Promise<GnuCashExportResult> =>
  invoke('export_gnucash', { input: { path } });

export const refreshPrices = (): Promise<PricesResult> => invoke('refresh_prices');

export const fetchAccountDetail = (
  guid: string,
  page: number,
  pageSize: number,
  dateFrom?: string,
  dateTo?: string,
  search?: string,
  sortBy?: TxSortKey,
  sortDir?: SortDir
): Promise<AccountDetailResult> =>
  invoke('fetch_account_detail', {
    guid,
    page,
    pageSize,
    dateFrom: dateFrom ?? null,
    dateTo: dateTo ?? null,
    search: search ?? null,
    sortBy: sortBy ?? null,
    sortDir: sortDir ?? null
  });

export const fetchAccountBalanceTrend = (guid: string): Promise<AccountBalancePoint[]> =>
  invoke('fetch_account_balance_trend', { guid });

// ── Reports ──

export interface ProfitLossRow {
  accountGuid: string;
  accountName: string;
  accountDescription?: string;
  accountCode: string | null;
  indent: number;
  currentAmount: number;
  previousAmount: number;
  changePct: number | null;
}

export interface ProfitLossReport {
  dateFrom: string;
  dateTo: string;
  prevDateFrom: string;
  prevDateTo: string;
  reportCurrency: string;
  incomeRows: ProfitLossRow[];
  incomeRootGuid: string | null;
  totalIncome: number;
  totalIncomePrev: number;
  expenseRows: ProfitLossRow[];
  expenseRootGuid: string | null;
  totalExpenses: number;
  totalExpensesPrev: number;
  netIncome: number;
  netIncomePrev: number;
  netIncomeChangePct: number | null;
  missingFx: string[];
}

export const fetchPnlReport = (
  dateFrom: string,
  dateTo: string,
  prevDateFrom?: string,
  prevDateTo?: string
): Promise<ProfitLossReport> =>
  invoke('fetch_pnl_report', {
    dateFrom,
    dateTo,
    prevDateFrom: prevDateFrom ?? null,
    prevDateTo: prevDateTo ?? null
  });

export interface PnlTrendPoint {
  /** YYYY-MM */
  month: string;
  income: number;
  expenses: number;
}

export const fetchPnlTrend = (dateFrom: string, dateTo: string): Promise<PnlTrendPoint[]> =>
  invoke('fetch_pnl_trend', { dateFrom, dateTo });

export interface MissingPrice {
  mnemonic: string;
  commodityGuid: string | null;
}

interface SymbolOverride {
  commodityGuid: string;
  mnemonic: string;
  namespace: string;
  fullName: string | null;
  symbol: string | null;
}

export const fetchSymbolOverrides = (): Promise<SymbolOverride[]> =>
  invoke('fetch_symbol_overrides');

export const setSymbolOverride = (commodityGuid: string, symbol: string): Promise<SymbolOverride> =>
  invoke('set_symbol_override', { commodityGuid, symbol });

export const deleteSymbolOverride = (commodityGuid: string): Promise<void> =>
  invoke('delete_symbol_override', { commodityGuid });

// ── Budgets ──

interface Budget {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountDescription?: string;
  accountCode: string | null;
  accountType: string;
  year: number;
  amount: number;
  createdAt: string;
}

export interface BudgetActualRow {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountDescription?: string;
  accountCode: string | null;
  accountType: string;
  budgetAmount: number;
  actualAmount: number;
  remaining: number;
  percentage: number;
  rolloverBalance: number;
  expectedYtd: number;
  pacingPercentage: number;
  delta: number;
  elapsedMonths: number;
  effectiveTotal: number;
}

export interface MonthlySlice {
  month: number;
  actualAmount: number;
  cumulativeActual: number;
  cumulativeExpected: number;
  delta: number;
  pacingPercentage: number;
}

export interface BudgetYearSummary {
  budgetAmount: number;
  rolloverBalance: number;
  effectiveTotal: number;
  actualAmount: number;
  expectedYtd: number;
  remaining: number;
  percentage: number;
  pacingPercentage: number;
  delta: number;
  elapsedMonths: number;
}

interface BudgetDetail {
  budget: Budget;
  summary: BudgetYearSummary;
  monthlyEntries: MonthlySlice[];
}

export const setBudgetYearAmount = (
  accountGuid: string,
  year: number,
  amount: number
): Promise<Budget> => invoke('set_budget_year_amount', { accountGuid, year, amount });

export const fetchBudgetYears = (accountGuid: string): Promise<number[]> =>
  invoke('fetch_budget_years', { accountGuid });

export const deleteBudgetYear = (accountGuid: string, year: number): Promise<void> =>
  invoke('delete_budget_year', { accountGuid, year });

export const fetchBudgetVsActual = (year: number): Promise<BudgetActualRow[]> =>
  invoke('fetch_budget_vs_actual', { year });

export const fetchBudgetDetail = (accountGuid: string, year: number): Promise<BudgetDetail> =>
  invoke('fetch_budget_detail', {
    accountGuid,
    year
  });

export interface BudgetDateRange {
  earliestDate: string | null;
  latestDate: string | null;
}

export const fetchBudgetDateRange = (): Promise<BudgetDateRange> =>
  invoke('fetch_budget_date_range');

// ── Portfolio ──

interface HoldingAccount {
  accountGuid: string;
  accountName: string;
  accountDescription: string | null;
  accountCode: string | null;
  shares: number;
  costBasis: number;
}

export interface PortfolioHolding {
  commodityGuid: string;
  mnemonic: string;
  fullName: string | null;
  namespace: string;
  shares: number;
  costBasis: number;
  marketValue: number;
  avgCostPerShare: number;
  gainLoss: number;
  xirr: number | null;
  allocationPct: number;
  price: number | null;
  prevPrice: number | null;
  priceDate: string | null;
  accounts: HoldingAccount[];
}

export interface PortfolioValuePoint {
  date: string;
  marketValue: number;
  costBasis: number;
}

interface PortfolioData {
  totalMarketValue: number;
  totalCostBasis: number;
  totalGainLoss: number;
  totalXirr: number | null;
  lastRefreshed: string | null;
  holdings: PortfolioHolding[];
  valueTrend: PortfolioValuePoint[];
}

export interface HoldingTransaction {
  guid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  accountName: string;
  accountDescription: string | null;
  accountCode: string | null;
  sharesDelta: number;
  valueDelta: number;
  runningShares: number;
}

interface HoldingDetail {
  holding: PortfolioHolding;
  priceHistory: PriceHistoryEntry[];
  valueTrend: PortfolioValuePoint[];
  transactions: HoldingTransaction[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

export const fetchPortfolioData = (): Promise<PortfolioData> => invoke('fetch_portfolio_data');

export const fetchHoldingDetail = (
  commodityGuid: string,
  page: number,
  pageSize: number,
  search?: string
): Promise<HoldingDetail> =>
  invoke('fetch_holding_detail', {
    commodityGuid,
    page,
    pageSize,
    search: search ?? null
  });

/** Saved rebalance target for one commodity (portfolio rebalance calculator). */
interface AllocationTarget {
  commodityGuid: string;
  targetPct: number;
}

export const fetchAllocationTargets = (): Promise<AllocationTarget[]> =>
  invoke('fetch_allocation_targets');

export const saveAllocationTargets = (targets: AllocationTarget[]): Promise<AllocationTarget[]> =>
  invoke('save_allocation_targets', { targets });

// ── Reconciliation (statement workflow for cash-like accounts) ──

export interface ReconcileCandidate {
  splitGuid: string;
  txGuid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  memo?: string | null;
  /** Signed display amount for the reconciled account. */
  amount: number;
  /** Normalized 'n' | 'c' (source NULL reads as 'n'). */
  reconcileState: string;
  /** Counter-account labels of the transaction, largest magnitude first. */
  counterAccounts: string[];
}

interface ReconcileInfo {
  accountGuid: string;
  accountName: string;
  accountDescription: string | null;
  accountType: string;
  commodity: string | null;
  statementDate: string;
  /** Sum of 'y' splits at or before the statement date. */
  startingBalance: number;
  /** Unreconciled splits at or before the statement date, oldest first. */
  candidates: ReconcileCandidate[];
  /** Post date of the most recent reconciled batch; null when never reconciled. */
  lastReconciledDate: string | null;
  /** Splits in that batch, what `reopenLastReconciliation` returns to cleared. */
  lastReconciledCount: number;
}

export interface ReconcileAccountSummary {
  accountGuid: string;
  /** Splits still 'n' (never cleared) in the visibility window. */
  unreconciledCount: number;
  /** Splits saved as 'c' (progress not yet finished). */
  clearedCount: number;
  /** Signed display total of the 'n' splits, in the account's commodity. */
  unclearedTotal: number;
  lastReconciledDate: string | null;
}

interface SplitStateUpdate {
  splitGuid: string;
  state: string;
}

interface MarkReconcileInput {
  accountGuid: string;
  statementDate: string;
  updates: SplitStateUpdate[];
}

interface MarkReconcileResult {
  updated: number;
}

interface FinishReconcileInput {
  accountGuid: string;
  statementDate: string;
  endingBalance: number;
  updates: SplitStateUpdate[];
}

interface FinishReconcileResult {
  reconciledCount: number;
  startingBalance: number;
  endingBalance: number;
}

export interface ReopenReconcileResult {
  reopenedCount: number;
  /** Post date of the batch before the reopened one; null for a first reconcile. */
  fromDate: string | null;
  throughDate: string;
}

export const fetchReconcileInfo = (
  accountGuid: string,
  statementDate: string
): Promise<ReconcileInfo> => invoke('fetch_reconcile_info', { accountGuid, statementDate });

export const markReconcileSplits = (
  accountGuid: string,
  statementDate: string,
  updates: { splitGuid: string; state: string }[]
): Promise<MarkReconcileResult> =>
  invoke('mark_reconcile_splits', {
    input: { accountGuid, statementDate, updates } satisfies MarkReconcileInput
  });

export const finishReconciliation = (
  accountGuid: string,
  statementDate: string,
  endingBalance: number,
  updates: { splitGuid: string; state: string }[]
): Promise<FinishReconcileResult> =>
  invoke('finish_reconciliation', {
    input: { accountGuid, statementDate, endingBalance, updates } satisfies FinishReconcileInput
  });

export const reopenLastReconciliation = (accountGuid: string): Promise<ReopenReconcileResult> =>
  invoke('reopen_last_reconciliation', { accountGuid });

export const fetchReconcileSummaries = (): Promise<ReconcileAccountSummary[]> =>
  invoke('fetch_reconcile_summaries');

// ── Global search (Ctrl+K palette) ──

interface SearchAccount {
  guid: string;
  name: string;
  fullPath: string;
  accountType: string;
  code: string | null;
  description: string | null;
}

interface SearchTransaction {
  guid: string;
  postDate: string;
  description: string | null;
  num: string | null;
}

interface SearchHolding {
  commodityGuid: string;
  mnemonic: string;
  fullName: string | null;
  namespace: string;
}

interface SearchBudget {
  accountGuid: string;
  accountName: string;
  /** Most recent year with a budget row for the account. */
  year: number;
}

interface GlobalSearchResult {
  accounts: SearchAccount[];
  transactions: SearchTransaction[];
  holdings: SearchHolding[];
  budgets: SearchBudget[];
}

export const searchGlobal = (query: string, limitPerGroup?: number): Promise<GlobalSearchResult> =>
  invoke('search_global', { query, limitPerGroup: limitPerGroup ?? null });

/**
 * Payload type of every backend command. `invoke` is typed against it, so a
 * misspelled command or a wrapper whose return type disagrees with the map
 * fails type-checking; the dev fixture backend (`$lib/dev/mock-invoke.ts`)
 * implements one handler per key.
 */
export type CommandPayloads = {
  add_attachments: AttachmentInfo[];
  clear_app_db: void;
  close_book: CloseBookResult;
  create_account: string;
  create_book_at: EmptyBookResult;
  create_import_rule: ImportRule;
  create_schedule: Schedule;
  create_transaction: string;
  csv_commit: CsvCommitResult;
  csv_detect: CsvDetectResult;
  csv_preview: CsvPreviewResult;
  delete_account: void;
  delete_attachment: void;
  delete_budget_year: void;
  delete_csv_preset: void;
  delete_import_rule: void;
  delete_schedule: void;
  delete_symbol_override: void;
  delete_transaction: void;
  export_accounts_csv: void;
  export_attachment: void;
  export_gnucash: GnuCashExportResult;
  export_transactions_csv: void;
  fetch_account_balance_trend: AccountBalancePoint[];
  fetch_account_detail: AccountDetailResult;
  fetch_accounts: AccountsResult;
  fetch_active_book: ActiveBook;
  fetch_allocation_targets: AllocationTarget[];
  fetch_attachments: AttachmentInfo[];
  fetch_book_currencies: BookCurrency[];
  fetch_budget_date_range: BudgetDateRange;
  fetch_budget_detail: BudgetDetail;
  fetch_budget_vs_actual: BudgetActualRow[];
  fetch_budget_years: number[];
  fetch_holding_detail: HoldingDetail;
  fetch_home_data: HomeDataResult;
  fetch_import_rules: ImportRule[];
  fetch_nav_badges: NavBadges;
  move_import_rule: ImportRule[];
  fetch_nav_roots: NavRoot[];
  fetch_pnl_report: ProfitLossReport;
  fetch_pnl_trend: PnlTrendPoint[];
  fetch_portfolio_data: PortfolioData;
  fetch_reconcile_info: ReconcileInfo;
  fetch_reconcile_summaries: ReconcileAccountSummary[];
  fetch_schedule: Schedule;
  fetch_schedules: Schedule[];
  fetch_symbol_overrides: SymbolOverride[];
  fetch_transaction: TransactionDetailResult | null;
  fetch_transaction_heatmap: HeatmapPoint[];
  fetch_transactions: TransactionsResult;
  finish_reconciliation: FinishReconcileResult;
  get_book_settings: BookSettings;
  import_gnucash: ImportResult;
  mark_reconcile_splits: MarkReconcileResult;
  open_attachment: void;
  open_book: EmptyBookResult;
  post_schedule_occurrence: string;
  preview_gnucash_import: GnuCashPreview;
  refresh_prices: PricesResult;
  reopen_last_reconciliation: ReopenReconcileResult;
  save_allocation_targets: AllocationTarget[];
  save_book_as: EmptyBookResult;
  search_global: GlobalSearchResult;
  set_base_currency: EmptyBookResult;
  set_book_setting: BookSettings;
  set_budget_year_amount: Budget;
  set_schedule_active: Schedule;
  set_symbol_override: SymbolOverride;
  skip_schedule_occurrence: string | null;
  update_account: void;
  update_import_rule: ImportRule;
  update_schedule: Schedule;
  update_transaction: void;
};

export type Command = keyof CommandPayloads;
