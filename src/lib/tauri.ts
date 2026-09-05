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

export interface ImportLogDetails {
  id: number;
  filename: string;
  filePath?: string;
  importedAt: string;
  sha256: string | null;
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
}

interface SplitInfo {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountType: string;
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

export interface AccountInfo {
  name: string;
}

interface Filters {
  search?: string;
  dateFrom?: string;
  dateTo?: string;
  accountName?: string;
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

interface TransactionDetail {
  guid: string;
  postDate: string;
  description: string | null;
  num?: string | null;
  notes?: string | null;
  splits: SplitDetail[];
  totalAmount: number;
}

interface TransactionDetailResult {
  transaction: TransactionDetail;
  prevGuid?: string | null;
  nextGuid?: string | null;
}

export interface AccountBalancePoint {
  month: string;
  balance: number;
}

/**
 * Response of `refresh_prices`. The backend also returns the full price list
 * and a `fetchedAt` timestamp; the UI only consumes per-commodity errors.
 */
interface PricesResult {
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
  descendantCount: number;
  childAccounts: ChildAccountInfo[];
}

/** The viewed account's parent; `null` for top-level accounts. */
export interface ParentAccountInfo {
  guid: string;
  name: string;
  description: string | null;
  balance: number;
}

interface AccountSummary {
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

interface DataInfo {
  dbPath: string;
}

interface ImportHistoryPage {
  logs: ImportLogDetails[];
  total: number;
  pageSize: number;
  totalPages: number;
}

export const getDataInfo = (): Promise<DataInfo> => invoke('get_data_info');

export const fetchImportHistory = (page: number, pageSize: number): Promise<ImportHistoryPage> =>
  invoke('fetch_import_history', { page, pageSize });

export const importGnucash = (path: string, filename: string): Promise<ImportResult> =>
  invoke('import_gnucash', { path, filename });

export const reimportLast = (): Promise<ImportResult> => invoke('reimport_last');

export const fetchAccounts = (asOfDate?: string): Promise<AccountsResult> =>
  invoke('fetch_accounts', { asOfDate: asOfDate ?? null });

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

export const refreshPrices = (): Promise<PricesResult> => invoke('refresh_prices');

export const fetchAccountDetail = (
  guid: string,
  page: number,
  pageSize: number,
  dateFrom?: string,
  dateTo?: string,
  search?: string,
  sortBy?: string,
  sortDir?: string
): Promise<AccountDetailResult> =>
  invoke('fetch_account_detail', {
    guid,
    page,
    pageSize,
    dateFrom,
    dateTo,
    search,
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
  incomeRows: ProfitLossRow[];
  incomeRootGuid: string | null;
  totalIncome: number;
  totalIncomePrev: number;
  expenseRows: ProfitLossRow[];
  expenseRootGuid: string | null;
  totalExpenses: number;
  totalExpensesPrev: number;
  netIncome: number;
}

export const generatePnlReport = (
  dateFrom: string,
  dateTo: string,
  prevDateFrom?: string,
  prevDateTo?: string
): Promise<ProfitLossReport> =>
  invoke('generate_pnl_report', {
    dateFrom,
    dateTo,
    prevDateFrom: prevDateFrom ?? null,
    prevDateTo: prevDateTo ?? null
  });

// ── Budgets ──

export interface Budget {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountDescription?: string;
  accountType: string;
  year: number;
  amount: number;
  notes: string | null;
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
  year: number;
  amount: number;
  summary: BudgetYearSummary;
  monthlyEntries: MonthlySlice[];
}

export const setBudgetYearAmount = (
  accountGuid: string,
  year: number,
  amount: number,
  notes?: string
): Promise<Budget> =>
  invoke('set_budget_year_amount', { accountGuid, year, amount, notes: notes ?? null });

export const setBudgetYearAmounts = (
  accountGuid: string,
  startYear: number,
  endYear: number,
  amount: number,
  notes?: string
): Promise<void> =>
  invoke('set_budget_year_amounts', {
    accountGuid,
    startYear,
    endYear,
    amount,
    notes: notes ?? null
  });

export const deleteBudgetYear = (accountGuid: string, year: number): Promise<void> =>
  invoke('delete_budget_year', { accountGuid, year });

export const getBudgetVsActual = (year: number): Promise<BudgetActualRow[]> =>
  invoke('get_budget_vs_actual', { year });

export const getBudgetDetail = (accountGuid: string, year: number): Promise<BudgetDetail> =>
  invoke('get_budget_detail', {
    accountGuid,
    year
  });

export interface BudgetDateRange {
  earliestDate: string | null;
  latestDate: string | null;
}

export const getBudgetDateRange = (): Promise<BudgetDateRange> => invoke('get_budget_date_range');

// ── Portfolio ──

interface HoldingAccount {
  accountGuid: string;
  accountName: string;
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
  accountName: string;
  sharesDelta: number;
  valueDelta: number;
}

export interface HoldingDetail {
  holding: PortfolioHolding;
  priceHistory: PriceHistoryEntry[];
  valueTrend: PortfolioValuePoint[];
  transactions: HoldingTransaction[];
}

export const fetchPortfolioData = (): Promise<PortfolioData> => invoke('fetch_portfolio_data');

export const fetchHoldingDetail = (commodityGuid: string): Promise<HoldingDetail> =>
  invoke('fetch_holding_detail', { commodityGuid });
