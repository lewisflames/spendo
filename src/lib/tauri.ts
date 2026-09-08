import { invoke } from '@tauri-apps/api/core';

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

interface ChangeHistoryPage {
  entries: ChangeEntry[];
  total: number;
  pageSize: number;
  totalPages: number;
  /** Newest entry eligible for linear undo, if any. */
  revertibleId: number | null;
}

interface RevertResult {
  revertedId: number;
  revertId: number;
  summary: DataSummary;
}

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
  /** Cumulative income minus expenses as of the requested date. */
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
interface PricesResult {
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

export const fetchHomeData = (): Promise<HomeDataResult> => invoke('fetch_home_data');

export const clearAppDb = (): Promise<void> => invoke('clear_app_db');

/** Writes text (e.g. a CSV export) to a user-chosen path. */
export const writeTextFile = (path: string, contents: string): Promise<void> =>
  invoke('write_text_file', { path, contents });

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

export const openBook = (path: string): Promise<EmptyBookResult> => invoke('open_book', { path });

export const createBookAt = (path: string, currency: string): Promise<EmptyBookResult> =>
  invoke('create_book_at', { path, currency });

export const saveBookAs = (path: string): Promise<EmptyBookResult> =>
  invoke('save_book_as', { path });

interface DataInfo {
  dbPath: string;
}

export const fetchDataInfo = (): Promise<DataInfo> => invoke('fetch_data_info');

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

export const getBookSettings = (): Promise<BookSettings> => invoke('get_book_settings');

export const setBookSetting = (key: string, value: string): Promise<BookSettings> =>
  invoke('set_book_setting', { key, value });

export const fetchChangeHistory = (page: number, pageSize: number): Promise<ChangeHistoryPage> =>
  invoke('fetch_change_history', { page, pageSize });

export const revertHistoryEntry = (id: number): Promise<RevertResult> =>
  invoke('revert_history_entry', { id });

export const importGnucash = (path: string, filename: string): Promise<ImportResult> =>
  invoke('import_gnucash', { path, filename });

export const reimportLast = (): Promise<ImportResult> => invoke('reimport_last');

export const setBaseCurrency = (currency: string): Promise<EmptyBookResult> =>
  invoke('set_base_currency', { currency });

export const fetchAccounts = (asOfDate?: string): Promise<AccountsResult> =>
  invoke('fetch_accounts', { asOfDate: asOfDate ?? null });

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
  invoke('create_transaction', { input });

export const updateTransaction = (guid: string, input: TxInput): Promise<void> =>
  invoke('update_transaction', { guid, input });

export const deleteTransaction = (guid: string): Promise<void> =>
  invoke('delete_transaction', { guid });

export const fetchTransactions = (
  page: number,
  pageSize: number,
  filters?: Filters,
  sortBy?: string,
  sortDir?: string
): Promise<TransactionsResult> =>
  invoke('fetch_transactions', {
    page,
    pageSize,
    filters: filters ?? null,
    sortBy: sortBy ?? null,
    sortDir: sortDir ?? null
  });

export const fetchAccountNames = (): Promise<AccountInfo[]> => invoke('fetch_account_names');

export const fetchTransactionHeatmap = (filters?: Filters): Promise<HeatmapPoint[]> =>
  invoke('fetch_transaction_heatmap', { filters: filters ?? null });

/** `null` when no transaction with that GUID exists. */
export const fetchTransaction = (guid: string): Promise<TransactionDetailResult | null> =>
  invoke('fetch_transaction', { guid });

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
  invoke('fetch_export_transactions');

export const fetchExportAccounts = (): Promise<ExportAccountRow[]> =>
  invoke('fetch_export_accounts');

export const refreshPrices = (): Promise<PricesResult> => invoke('refresh_prices');

export const fetchAccountDetail = (
  guid: string,
  page: number,
  pageSize: number,
  dateFrom?: string,
  dateTo?: string,
  search?: string,
  sortBy?: string,
  sortDir?: string,
  asOf?: string
): Promise<AccountDetailResult> =>
  invoke('fetch_account_detail', {
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
  invoke('fetch_account_balance_trend', { guid });

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

interface PortfolioData {
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
  invoke('fetch_holding_detail', { commodityGuid, page, pageSize, search: search ?? null });
