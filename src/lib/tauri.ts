import { invoke } from '@tauri-apps/api/core';

export interface HomeDataResult {
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
export type SortDir = 'asc' | 'desc';

/** One row of the unified changes history (imports + manual edits). */
export interface ChangeEntry {
  id: number;
  occurredAt: string;
  kind: string;
  label: string;
  source: string;
  transactionsAffected: number;
  accountsAffected: number;
  revertsId: number | null;
  revertedBy: number | null;
}

export interface ChangeHistoryPage {
  entries: ChangeEntry[];
  total: number;
  pageSize: number;
  totalPages: number;
  /** Newest entry eligible for linear undo, if any. */
  revertibleId: number | null;
}

export interface RevertResult {
  revertedId: number;
  revertId: number;
  summary: DataSummary;
}

export interface ImportResult {
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

export interface TypeSummary {
  count: number;
  totalBalance: number;
}

export interface AccountsResult {
  trees: AccountNode[];
  byType: Record<string, TypeSummary>;
  totalAccounts: number;
  reportCurrency: string;
  /** Cumulative income minus expenses as of the requested date. */
  retainedEarnings: number;
  /** Market minus cost for priced investments. */
  unrealizedGains: number;
  /** Assets − (liabilities + equity + unrealized + retained); ~0 when balanced. */
  balanceCheck: number;
  missingFx: string[];
  missingPrices: MissingPrice[];
}

export interface SplitInfo {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountType: string;
  amount: number;
  memo?: string | null;
  accountDescription?: string | null;
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

export interface AccountInfo {
  name: string;
}

export interface Filters {
  search?: string;
  dateFrom?: string;
  dateTo?: string;
  accountName?: string;
  accountGuid?: string;
}

export interface TransactionsResult {
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
}

export interface TransactionDetail {
  guid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitDetail[];
  totalAmount: number;
}

export interface TransactionDetailResult {
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
  /** Other accounts involved, outside the viewed account's subtree. */
  counterAccounts: string[];
  /** Signed effect on the viewed subtree, in report currency at the tx date. */
  amount: number;
  runningBalance: number;
  /** GnuCash reconcile state of the viewed subtree's splits: 'n'/'c'/'y'. */
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

/** The viewed account's parent; `null` for top-level accounts. */
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
  /** The viewed account's parent, for the pie's parent-ratio label. */
  parent: ParentAccountInfo | null;
}

export const fetchHomeData = (): Promise<HomeDataResult> =>
  invoke<HomeDataResult>('fetch_home_data');

export const clearAppDb = (): Promise<void> => invoke<void>('clear_app_db');

/** Writes text (e.g. a CSV export) to a user-chosen path. */
export const writeTextFile = (path: string, contents: string): Promise<void> =>
  invoke<void>('write_text_file', { path, contents });

// ── Books (document model: one active `.spendo` file opened in place) ──

/** Active book path, or `null` while book-less (fresh install). */
export interface ActiveBook {
  path: string | null;
}

export const fetchActiveBook = (): Promise<ActiveBook> => invoke<ActiveBook>('fetch_active_book');

export interface EmptyBookResult {
  hasData: boolean;
  summary: DataSummary;
}

export const openBook = (path: string): Promise<EmptyBookResult> =>
  invoke<EmptyBookResult>('open_book', { path });

export const createBookAt = (path: string, currency: string): Promise<EmptyBookResult> =>
  invoke<EmptyBookResult>('create_book_at', { path, currency });

export const saveBookAs = (path: string): Promise<EmptyBookResult> =>
  invoke<EmptyBookResult>('save_book_as', { path });

export interface DataInfo {
  dbPath: string;
}

export const fetchDataInfo = (): Promise<DataInfo> => invoke<DataInfo>('fetch_data_info');

// ── Book-scoped settings (travel with the `.spendo` file) ──

export type PnlPeriodId = 'this_month' | 'this_quarter' | 'this_year' | 'last_12_months';
export type PriceAutoRefresh = 'off' | 'on_startup';
export type CsvDelimiter = ',' | ';';
export type ExportDateFormat = 'iso' | 'locale';

export interface BookSettings {
  fiscalYearStartMonth: number;
  defaultPnlPeriod: PnlPeriodId;
  priceAutoRefresh: PriceAutoRefresh;
  exportCsvDelimiter: CsvDelimiter;
  exportDateFormat: ExportDateFormat;
}

export const getBookSettings = (): Promise<BookSettings> =>
  invoke<BookSettings>('get_book_settings');

export const setBookSetting = (key: string, value: string): Promise<BookSettings> =>
  invoke<BookSettings>('set_book_setting', { key, value });

export const fetchChangeHistory = (page: number, pageSize: number): Promise<ChangeHistoryPage> =>
  invoke<ChangeHistoryPage>('fetch_change_history', { page, pageSize });

export const revertHistoryEntry = (id: number): Promise<RevertResult> =>
  invoke<RevertResult>('revert_history_entry', { id });

export const importGnucash = (path: string, filename: string): Promise<ImportResult> =>
  invoke<ImportResult>('import_gnucash', { path, filename });

export const reimportLast = (): Promise<ImportResult> => invoke<ImportResult>('reimport_last');

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

export interface CsvPreviewInput {
  path: string;
  encoding?: string | null;
  delimiter?: string | null;
  hasHeader?: boolean | null;
  mapping: CsvMapping;
  targetAccountGuid: string;
  offsetAccountGuid: string;
  datePreference?: string | null;
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
}

export interface CsvPreviewRow {
  key: number;
  raw: string[];
  postDate: string | null;
  description: string | null;
  amount: number | null;
  valid: boolean;
  duplicate: boolean;
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
  entryId: number;
  label: string;
}

export const csvDetect = (path: string): Promise<CsvDetectResult> =>
  invoke<CsvDetectResult>('csv_detect', { path });

export const csvPreview = (input: CsvPreviewInput): Promise<CsvPreviewResult> =>
  invoke<CsvPreviewResult>('csv_preview', { input });

export const csvCommit = (
  input: CsvPreviewInput & { selectedKeys: number[] }
): Promise<CsvCommitResult> => invoke<CsvCommitResult>('csv_commit', { input });

export const setBaseCurrency = (currency: string): Promise<EmptyBookResult> =>
  invoke<EmptyBookResult>('set_base_currency', { currency });

export const fetchAccounts = (asOf?: string): Promise<AccountsResult> =>
  invoke<AccountsResult>('fetch_accounts', { asOf: asOf ?? null });

export interface BookCurrency {
  guid: string;
  mnemonic: string;
}

export const fetchBookCurrencies = (): Promise<BookCurrency[]> =>
  invoke<BookCurrency[]>('fetch_book_currencies');

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
  invoke<string>('create_account', { input });

export const updateAccount = (guid: string, input: AccountInput): Promise<void> =>
  invoke<void>('update_account', { guid, input });

export const deleteAccount = (guid: string): Promise<void> =>
  invoke<void>('delete_account', { guid });

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
}

export const createTransaction = (input: TxInput): Promise<string> =>
  invoke<string>('create_transaction', { input });

export const updateTransaction = (guid: string, input: TxInput): Promise<void> =>
  invoke<void>('update_transaction', { guid, input });

export const deleteTransaction = (guid: string): Promise<void> =>
  invoke<void>('delete_transaction', { guid });

export const fetchTransactions = (
  page: number,
  pageSize: number,
  filters?: Filters,
  sortBy?: TxSortKey,
  sortDir?: SortDir
): Promise<TransactionsResult> =>
  invoke<TransactionsResult>('fetch_transactions', {
    page,
    pageSize,
    filters: filters ?? null,
    sortBy: sortBy ?? null,
    sortDir: sortDir ?? null
  });

export const fetchAccountNames = (): Promise<AccountInfo[]> =>
  invoke<AccountInfo[]>('fetch_account_names');

export const fetchTransactionHeatmap = (filters?: Filters): Promise<HeatmapPoint[]> =>
  invoke<HeatmapPoint[]>('fetch_transaction_heatmap', { filters: filters ?? null });

/** `null` when no transaction with that GUID exists. */
export const fetchTransaction = (guid: string): Promise<TransactionDetailResult | null> =>
  invoke<TransactionDetailResult | null>('fetch_transaction', { guid });

// ── Recurring schedules (manual remind + post; in-app reminders only) ──

export type ScheduleFrequency =
  'daily' | 'weekly' | 'biweekly' | 'monthly' | 'quarterly' | 'yearly';

export interface ScheduleSplit {
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

export const fetchSchedules = (): Promise<Schedule[]> => invoke<Schedule[]>('fetch_schedules');

export const fetchSchedule = (guid: string): Promise<Schedule> =>
  invoke<Schedule>('fetch_schedule', { guid });

export const createSchedule = (input: ScheduleInput): Promise<Schedule> =>
  invoke<Schedule>('create_schedule', { input });

export const updateSchedule = (guid: string, input: ScheduleInput): Promise<Schedule> =>
  invoke<Schedule>('update_schedule', { guid, input });

export const deleteSchedule = (guid: string): Promise<void> =>
  invoke<void>('delete_schedule', { guid });

export const setScheduleActive = (guid: string, active: boolean): Promise<Schedule> =>
  invoke<Schedule>('set_schedule_active', { guid, active });

/** Posts the due occurrence; resolves with the new transaction GUID. */
export const postScheduleOccurrence = (guid: string): Promise<string> =>
  invoke<string>('post_schedule_occurrence', { guid });

/** Skips the due occurrence; resolves with the new pointer (`null` = completed). */
export const skipScheduleOccurrence = (guid: string): Promise<string | null> =>
  invoke<string | null>('skip_schedule_occurrence', { guid });

/** One row per split for the settings CSV export (GnuCash multi-split shape). */
export interface ExportSplitRow {
  txGuid: string;
  splitGuid: string;
  /** YYYY-MM-DD */
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  commodity?: string | null;
  memo?: string | null;
  fullAccountName: string;
  accountName: string;
  amount: number;
  reconcileState?: string | null;
  price: number;
}

/** One row per account in parent-before-child order for the settings CSV export. */
export interface ExportAccountRow {
  guid: string;
  parentGuid: string | null;
  accountType: string;
  fullName: string;
  name: string;
  code: string | null;
  description: string | null;
  commodity: string | null;
  commodityNamespace: string | null;
  placeholder: boolean;
}

export const fetchExportTransactions = (): Promise<ExportSplitRow[]> =>
  invoke<ExportSplitRow[]>('fetch_export_transactions');

export const fetchExportAccounts = (): Promise<ExportAccountRow[]> =>
  invoke<ExportAccountRow[]>('fetch_export_accounts');

export const refreshPrices = (): Promise<PricesResult> => invoke<PricesResult>('refresh_prices');

export const fetchAccountDetail = (
  guid: string,
  page: number,
  pageSize: number,
  dateFrom?: string,
  dateTo?: string,
  search?: string,
  sortBy?: TxSortKey,
  sortDir?: SortDir,
  asOf?: string
): Promise<AccountDetailResult> =>
  invoke<AccountDetailResult>('fetch_account_detail', {
    guid,
    page,
    pageSize,
    dateFrom: dateFrom ?? null,
    dateTo: dateTo ?? null,
    search: search ?? null,
    sortBy: sortBy ?? null,
    sortDir: sortDir ?? null,
    asOf: asOf ?? null
  });

export const fetchAccountBalanceTrend = (guid: string): Promise<AccountBalancePoint[]> =>
  invoke<AccountBalancePoint[]>('fetch_account_balance_trend', { guid });

// ── Reports ──

export interface ProfitLossRow {
  accountGuid: string;
  accountName: string;
  accountDescription?: string;
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
  invoke<ProfitLossReport>('fetch_pnl_report', {
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
  invoke<PnlTrendPoint[]>('fetch_pnl_trend', { dateFrom, dateTo });

export interface MissingPrice {
  mnemonic: string;
  commodityGuid: string | null;
}

export interface SymbolOverride {
  commodityGuid: string;
  mnemonic: string;
  namespace: string;
  fullName: string | null;
  symbol: string | null;
}

export const fetchSymbolOverrides = (): Promise<SymbolOverride[]> =>
  invoke<SymbolOverride[]>('fetch_symbol_overrides');

export const setSymbolOverride = (commodityGuid: string, symbol: string): Promise<SymbolOverride> =>
  invoke<SymbolOverride>('set_symbol_override', { commodityGuid, symbol });

export const deleteSymbolOverride = (commodityGuid: string): Promise<void> =>
  invoke<void>('delete_symbol_override', { commodityGuid });

// ── Budgets ──

export interface Budget {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountDescription?: string;
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

export interface BudgetDetail {
  budget: Budget;
  summary: BudgetYearSummary;
  monthlyEntries: MonthlySlice[];
}

export const setBudgetYearAmount = (
  accountGuid: string,
  year: number,
  amount: number
): Promise<Budget> => invoke<Budget>('set_budget_year_amount', { accountGuid, year, amount });

export const fetchBudgetYears = (accountGuid: string): Promise<number[]> =>
  invoke<number[]>('fetch_budget_years', { accountGuid });

export const deleteBudgetYear = (accountGuid: string, year: number): Promise<void> =>
  invoke<void>('delete_budget_year', { accountGuid, year });

export const fetchBudgetVsActual = (year: number): Promise<BudgetActualRow[]> =>
  invoke<BudgetActualRow[]>('fetch_budget_vs_actual', { year });

export const fetchBudgetDetail = (accountGuid: string, year: number): Promise<BudgetDetail> =>
  invoke<BudgetDetail>('fetch_budget_detail', {
    accountGuid,
    year
  });

export interface BudgetDateRange {
  earliestDate: string | null;
  latestDate: string | null;
}

export const fetchBudgetDateRange = (): Promise<BudgetDateRange> =>
  invoke<BudgetDateRange>('fetch_budget_date_range');

// ── Portfolio ──

export interface HoldingAccount {
  accountGuid: string;
  accountName: string;
  accountDescription: string | null;
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
  gainLossPct: number;
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

export interface PortfolioData {
  totalMarketValue: number;
  totalCostBasis: number;
  totalGainLoss: number;
  totalGainLossPct: number;
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
  sharesDelta: number;
  valueDelta: number;
  runningShares: number;
}

export interface HoldingDetail {
  holding: PortfolioHolding;
  priceHistory: PriceHistoryEntry[];
  valueTrend: PortfolioValuePoint[];
  transactions: HoldingTransaction[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

export const fetchPortfolioData = (): Promise<PortfolioData> =>
  invoke<PortfolioData>('fetch_portfolio_data');

export const fetchHoldingDetail = (
  commodityGuid: string,
  page: number,
  pageSize: number,
  search?: string
): Promise<HoldingDetail> =>
  invoke<HoldingDetail>('fetch_holding_detail', {
    commodityGuid,
    page,
    pageSize,
    search: search ?? null
  });

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
}

export interface ReconcileInfo {
  accountGuid: string;
  accountName: string;
  accountType: string;
  commodity: string | null;
  statementDate: string;
  /** Sum of 'y' splits at or before the statement date. */
  startingBalance: number;
  /** Unreconciled splits at or before the statement date, oldest first. */
  candidates: ReconcileCandidate[];
}

interface SplitStateUpdate {
  splitGuid: string;
  state: string;
}

interface MarkReconcileInput {
  accountGuid: string;
  updates: SplitStateUpdate[];
}

export interface MarkReconcileResult {
  updated: number;
}

interface FinishReconcileInput {
  accountGuid: string;
  statementDate: string;
  endingBalance: number;
}

export interface FinishReconcileResult {
  reconciledCount: number;
  startingBalance: number;
  endingBalance: number;
}

export const fetchReconcileInfo = (
  accountGuid: string,
  statementDate: string
): Promise<ReconcileInfo> =>
  invoke<ReconcileInfo>('fetch_reconcile_info', { accountGuid, statementDate });

export const markReconcileSplits = (
  accountGuid: string,
  updates: { splitGuid: string; state: string }[]
): Promise<MarkReconcileResult> =>
  invoke<MarkReconcileResult>('mark_reconcile_splits', {
    input: { accountGuid, updates } satisfies MarkReconcileInput
  });

export const finishReconciliation = (
  accountGuid: string,
  statementDate: string,
  endingBalance: number
): Promise<FinishReconcileResult> =>
  invoke<FinishReconcileResult>('finish_reconciliation', {
    input: { accountGuid, statementDate, endingBalance } satisfies FinishReconcileInput
  });

// ── Global search (Ctrl+K palette) ──

export interface SearchAccount {
  guid: string;
  name: string;
  fullPath: string;
  accountType: string;
  code: string | null;
}

export interface SearchTransaction {
  guid: string;
  postDate: string;
  description: string | null;
  num: string | null;
}

export interface SearchHolding {
  commodityGuid: string;
  mnemonic: string;
  fullName: string | null;
  namespace: string;
}

export interface SearchBudget {
  accountGuid: string;
  accountName: string;
  /** Most recent year with a budget row for the account. */
  year: number;
}

export interface GlobalSearchResult {
  accounts: SearchAccount[];
  transactions: SearchTransaction[];
  holdings: SearchHolding[];
  budgets: SearchBudget[];
}

export const searchGlobal = (query: string, limitPerGroup?: number): Promise<GlobalSearchResult> =>
  invoke<GlobalSearchResult>('search_global', { query, limitPerGroup: limitPerGroup ?? null });
