/**
 * Fixture backend for `bun run dev` in a plain browser (no Tauri runtime).
 *
 * `$lib/tauri.ts` dynamically imports this module in dev when `isTauri()` is
 * false, so every page can load real-looking data without the desktop shell.
 * Handlers mirror the Rust commands' shapes; unknown commands throw with a
 * pointer to this file so gaps are loud instead of blank pages.
 *
 * State is intentionally in-memory and per-session: Vite HMR rebuilds reset the
 * book to the fixtures.
 */
import type {
  AccountNode,
  CsvPreset,
  DataSummary,
  Schedule,
  ScheduleFrequency,
  SplitDetail,
  TransactionDetail,
  TransactionRow,
  MockPayloads
} from '$lib/tauri';
import {
  ACCOUNTS,
  BUDGET_PLANS,
  COMMODITIES,
  CREDIT_NORMAL_TYPES,
  INVESTMENT_TYPES,
  MARKET_PRICES,
  REPORT_CURRENCY,
  SAMPLE_IMAGE_DATA_URL,
  SAMPLE_TEXT_DATA_URL,
  SCHEDULES,
  TRANSACTIONS,
  categoryOf,
  todayIso,
  type FixtureAccount,
  type FixtureScheduleDef,
  type FixtureSplit,
  type FixtureTx
} from './fixtures';

// ── state ──────────────────────────────────────────────────────────────────

interface AttachmentRecord {
  guid: string;
  txGuid: string;
  filename: string;
  mimeType: string;
  byteSize: number;
  createdAt: string;
  isImage: boolean;
}

interface TxInputLike {
  postDate: string;
  description?: string | null;
  num?: string | null;
  notes?: string | null;
  splits: {
    accountGuid: string;
    amount: number;
    quantity?: number | null;
    memo?: string | null;
  }[];
}

interface AccountInputLike {
  name: string;
  accountType: string;
  parentGuid?: string | null;
  code?: string | null;
  description?: string | null;
  commodityGuid?: string | null;
  placeholder: boolean;
}

interface CsvInputLike {
  targetAccountGuid: string;
  offsetAccountGuid: string;
  selectedKeys?: number[];
  rememberMapping?: boolean;
}

interface ImportRuleLike {
  id: number;
  priority: number;
  matchType: 'contains' | 'equals';
  pattern: string;
  accountGuid: string;
}

interface ScheduleInputLike {
  name: string;
  frequency: ScheduleFrequency;
  intervalMult?: number | null;
  startDate: string;
  endDate?: string | null;
  maxOccurrences?: number | null;
  description?: string | null;
  num?: string | null;
  notes?: string | null;
  active?: boolean;
  splits: { accountGuid: string; amount: number; memo?: string | null }[];
}

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

let nextId = 1;
const id = (prefix: string) => `${prefix}-${nextId++}`;

const nowIso = () => new Date().toISOString();

const state = {
  bookPath: '/home/demo/Books/demo.spendo' as string | null,
  baseCurrency: REPORT_CURRENCY,
  accounts: clone(ACCOUNTS),
  transactions: clone(TRANSACTIONS),
  schedules: [] as Schedule[],
  budgets: clone(BUDGET_PLANS),
  overrides: new Map<string, string>(),
  allocationTargets: new Map<string, number>(),
  attachments: [] as AttachmentRecord[],
  csvPresets: new Map<string, CsvPreset>(),
  importRules: [] as ImportRuleLike[],
  nextImportRuleId: 1,
  bookSettings: {
    fiscalYearStartMonth: 1,
    defaultPnlPeriod: 'this_year' as const,
    priceAutoRefresh: 'on_startup' as const,
    exportCsvDelimiter: ',' as const,
    exportDateFormat: 'iso' as const
  }
};

// ── generic helpers ────────────────────────────────────────────────────────

const account = (guid: string): FixtureAccount | undefined =>
  state.accounts.find((a) => a.guid === guid);

const visibleTxs = (): FixtureTx[] => state.transactions.filter((tx) => tx.postDate <= todayIso());

/** Display amount (credit-normal types read positive), mirroring `signed_amount_expr`. */
function splitAmount(split: FixtureSplit): number {
  const acc = account(split.accountGuid);
  if (!acc) return 0;
  return CREDIT_NORMAL_TYPES.has(acc.accountType) ? -split.raw : split.raw;
}

function splitRaw(accountType: string, amount: number): number {
  return CREDIT_NORMAL_TYPES.has(accountType) ? -amount : amount;
}

function subtreeGuids(guid: string): Set<string> {
  const out = new Set<string>([guid]);
  let added = true;
  while (added) {
    added = false;
    for (const acc of state.accounts) {
      if (acc.parentGuid && out.has(acc.parentGuid) && !out.has(acc.guid)) {
        out.add(acc.guid);
        added = true;
      }
    }
  }
  return out;
}

const directChildren = (guid: string): FixtureAccount[] =>
  state.accounts.filter((a) => a.parentGuid === guid && a.accountType !== 'ROOT');

function ownCash(guid: string): number {
  let sum = 0;
  for (const tx of visibleTxs()) {
    for (const split of tx.splits) {
      if (split.accountGuid === guid) sum += splitAmount(split);
    }
  }
  return sum;
}

function investmentTotals(guid: string): { shares: number; cost: number } {
  const guids = subtreeGuids(guid);
  let shares = 0;
  let cost = 0;
  for (const tx of visibleTxs()) {
    for (const split of tx.splits) {
      if (!guids.has(split.accountGuid)) continue;
      const acc = account(split.accountGuid);
      if (!acc || !INVESTMENT_TYPES.has(acc.accountType)) continue;
      shares += split.quantity ?? 0;
      cost += split.raw;
    }
  }
  return { shares, cost };
}

function marketPrice(mnemonic: string | null | undefined): number | null {
  if (!mnemonic) return null;
  return MARKET_PRICES[mnemonic]?.price ?? null;
}

function nodeBalance(guid: string): number {
  const acc = account(guid);
  if (!acc) return 0;
  if (INVESTMENT_TYPES.has(acc.accountType)) {
    const { shares, cost } = investmentTotals(guid);
    const price = marketPrice(acc.commodity);
    return price === null ? cost : shares * price;
  }
  let sum = ownCash(guid);
  for (const child of directChildren(guid)) sum += nodeBalance(child.guid);
  return sum;
}

function txCountFor(guid: string): number {
  const guids = subtreeGuids(guid);
  return visibleTxs().filter((tx) => tx.splits.some((s) => guids.has(s.accountGuid))).length;
}

function accountNode(acc: FixtureAccount): AccountNode {
  return {
    guid: acc.guid,
    name: acc.name,
    accountType: acc.accountType,
    code: acc.code,
    description: acc.description,
    placeholder: acc.placeholder,
    commodity: acc.commodity,
    commodityGuid: acc.commodityGuid,
    balance: nodeBalance(acc.guid),
    txCount: txCountFor(acc.guid),
    children: directChildren(acc.guid)
      .map(accountNode)
      .sort((a, b) => a.name.localeCompare(b.name))
  };
}

const CATEGORY_ROOTS = ['assets', 'liabilities', 'equity', 'income', 'expenses'] as const;

function cashTotalForCategory(category: string): number {
  const guids = new Set(
    state.accounts
      .filter((a) => categoryOf(a.accountType) === category && !INVESTMENT_TYPES.has(a.accountType))
      .map((a) => a.guid)
  );
  let sum = 0;
  for (const tx of visibleTxs()) {
    for (const split of tx.splits) {
      if (guids.has(split.accountGuid)) sum += splitAmount(split);
    }
  }
  return sum;
}

function unrealizedGains(): number {
  let total = 0;
  for (const acc of state.accounts) {
    if (!INVESTMENT_TYPES.has(acc.accountType)) continue;
    const { shares, cost } = investmentTotals(acc.guid);
    const price = marketPrice(acc.commodity);
    if (price !== null) total += shares * price - cost;
  }
  return total;
}

function dataSummary(): DataSummary {
  const txs = visibleTxs();
  const dates = txs.map((tx) => tx.postDate).sort();
  return {
    currency: state.baseCurrency,
    totalAccounts: state.accounts.filter((a) => a.accountType !== 'ROOT').length,
    totalTransactions: txs.length,
    earliestDate: dates[0] ?? null,
    latestDate: dates.at(-1) ?? null,
    netWorth: nodeBalance('assets') - nodeBalance('liabilities')
  };
}

function homeData(): MockPayloads['fetch_home_data'] {
  if (state.bookPath === null) return { hasData: false, summary: null };
  const summary = dataSummary();
  return { hasData: summary.totalTransactions > 0, summary };
}

function emptyBookResult(): MockPayloads['open_book'] {
  const summary = dataSummary();
  return { hasData: summary.totalTransactions > 0, summary };
}

/** Split GUID is `${txGuid}-${splitIndex}` — see `toSplitInfo`. */
function findSplit(
  splitGuid: string
): { tx: FixtureTx; split: FixtureSplit; index: number } | null {
  const cut = splitGuid.lastIndexOf('-');
  if (cut <= 0) return null;
  const txGuid = splitGuid.slice(0, cut);
  const index = Number(splitGuid.slice(cut + 1));
  const tx = state.transactions.find((t) => t.guid === txGuid);
  const split = tx?.splits[index];
  return tx && split ? { tx, split, index } : null;
}

const round2 = (n: number) => Math.round(n * 100) / 100;

function isoLocal(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${date.getFullYear()}-${month}-${day}`;
}

// ── accounts ───────────────────────────────────────────────────────────────

function accountsResult(): MockPayloads['fetch_accounts'] {
  const trees = CATEGORY_ROOTS.map((guid) => accountNode(account(guid)!)).filter(Boolean);
  const byType: MockPayloads['fetch_accounts']['byType'] = {};
  for (const category of ['ASSET', 'LIABILITY', 'EQUITY', 'INCOME', 'EXPENSE']) {
    const root = trees.find((t) => categoryOf(t.accountType) === category);
    byType[category] = {
      count: state.accounts.filter(
        (a) => a.accountType !== 'ROOT' && !a.placeholder && categoryOf(a.accountType) === category
      ).length,
      totalBalance: root?.balance ?? 0
    };
  }
  const assets = nodeBalance('assets');
  const liabilities = nodeBalance('liabilities');
  const equity = nodeBalance('equity');
  const unrealized = unrealizedGains();
  const retained = cashTotalForCategory('INCOME') - cashTotalForCategory('EXPENSE');
  const missingPrices = state.accounts
    .filter((a) => INVESTMENT_TYPES.has(a.accountType) && marketPrice(a.commodity) === null)
    .map((a) => ({ mnemonic: a.commodity ?? a.name, commodityGuid: a.commodityGuid }));
  return {
    trees,
    byType,
    totalAccounts: state.accounts.filter((a) => a.accountType !== 'ROOT').length,
    reportCurrency: state.baseCurrency,
    retainedEarnings: retained,
    unrealizedGains: unrealized,
    balanceCheck: assets - liabilities - equity - unrealized - retained,
    missingFx: [],
    missingPrices
  };
}

function toSplitDetail(tx: FixtureTx, split: FixtureSplit, index: number): SplitDetail {
  const acc = account(split.accountGuid);
  return {
    guid: `${tx.guid}-${index}`,
    accountGuid: split.accountGuid,
    accountName: acc?.name ?? 'Unknown account',
    accountType: acc?.accountType ?? 'ASSET',
    amount: splitAmount(split),
    memo: split.memo ?? null,
    quantity: split.quantity ?? splitAmount(split),
    commodity: acc?.commodity ?? null,
    reconciled: split.reconcileState === 'y',
    reconcileState: split.reconcileState ?? 'n',
    accountDescription: acc?.description ?? null
  };
}

function accountDetail(args: Record<string, unknown>): MockPayloads['fetch_account_detail'] {
  const guid = String(args.guid);
  const acc = account(guid);
  if (!acc) throw new Error(`[mock] Unknown account ${guid}`);
  const pageSize = Number(args.pageSize) || 25;
  const page = Math.max(1, Number(args.page) || 1);
  const dateFrom = args.dateFrom ? String(args.dateFrom) : null;
  const dateTo = args.dateTo ? String(args.dateTo) : null;
  const search = args.search ? String(args.search).toLowerCase() : null;
  const sortBy = (args.sortBy ? String(args.sortBy) : 'date') as 'date' | 'description' | 'amount';
  const sortDir = (args.sortDir === 'asc' ? 'asc' : 'desc') as 'asc' | 'desc';

  const inSubtree = subtreeGuids(guid);
  let rows = visibleTxs().filter((tx) => tx.splits.some((s) => inSubtree.has(s.accountGuid)));
  if (dateFrom) rows = rows.filter((tx) => tx.postDate >= dateFrom);
  if (dateTo) rows = rows.filter((tx) => tx.postDate <= dateTo);
  if (search) {
    rows = rows.filter(
      (tx) =>
        (tx.description ?? '').toLowerCase().includes(search) ||
        (tx.num ?? '').toLowerCase().includes(search) ||
        tx.splits.some((s) => (account(s.accountGuid)?.name ?? '').toLowerCase().includes(search))
    );
  }

  const delta = (tx: FixtureTx) =>
    tx.splits
      .filter((s) => inSubtree.has(s.accountGuid))
      .reduce((sum, s) => sum + splitAmount(s), 0);

  const ascending = [...rows].sort(
    (a, b) => a.postDate.localeCompare(b.postDate) || a.guid.localeCompare(b.guid)
  );
  let running = 0;
  const runningByGuid = new Map<string, { delta: number; running: number }>();
  for (const tx of ascending) {
    const d = delta(tx);
    running += d;
    runningByGuid.set(tx.guid, { delta: d, running });
  }

  const sorted = [...rows].sort((a, b) => {
    const dir = sortDir === 'asc' ? 1 : -1;
    if (sortBy === 'description') {
      return (a.description ?? '').localeCompare(b.description ?? '') * dir;
    }
    if (sortBy === 'amount') {
      return (delta(a) - delta(b)) * dir;
    }
    return (a.postDate.localeCompare(b.postDate) || a.guid.localeCompare(b.guid)) * dir;
  });

  const total = sorted.length;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(page, totalPages);
  const pageRows = sorted.slice((safePage - 1) * pageSize, safePage * pageSize);

  const transactions: MockPayloads['fetch_account_detail']['transactions'] = pageRows.map((tx) => {
    const entry = runningByGuid.get(tx.guid)!;
    const relevant = tx.splits.filter((s) => inSubtree.has(s.accountGuid));
    const counterAccounts = [
      ...new Set(
        tx.splits
          .filter((s) => !inSubtree.has(s.accountGuid))
          .map((s) => account(s.accountGuid)?.name ?? 'Unknown')
      )
    ];
    return {
      guid: tx.guid,
      postDate: tx.postDate,
      description: tx.description,
      num: tx.num ?? null,
      notes: tx.notes ?? null,
      splits: tx.splits.map((s, i) => toSplitDetail(tx, s, i)),
      counterAccounts,
      amount: entry.delta,
      runningBalance: entry.running,
      reconcileState: relevant[0]?.reconcileState ?? 'n'
    };
  });

  let totalIncome = 0;
  let totalExpenses = 0;
  for (const tx of rows) {
    const d = delta(tx);
    if (d >= 0) totalIncome += d;
    else totalExpenses += -d;
  }

  const parent = acc.parentGuid && acc.parentGuid !== 'root' ? account(acc.parentGuid) : null;
  return {
    account: {
      guid: acc.guid,
      name: acc.name,
      accountType: acc.accountType,
      code: acc.code,
      description: acc.description,
      placeholder: acc.placeholder,
      commodity: acc.commodity,
      commodityGuid: acc.commodityGuid,
      parentGuid: acc.parentGuid,
      balance: nodeBalance(acc.guid),
      descendantsCount: subtreeGuids(acc.guid).size - 1,
      childAccounts: directChildren(acc.guid).map((child) => ({
        guid: child.guid,
        name: child.name,
        description: child.description,
        code: child.code,
        commodity: child.commodity,
        commodityGuid: child.commodityGuid,
        accountType: child.accountType,
        balance: nodeBalance(child.guid),
        txCount: txCountFor(child.guid)
      }))
    },
    summary: { totalTransactions: rows.length, totalIncome, totalExpenses },
    transactions,
    total,
    page: safePage,
    pageSize,
    totalPages,
    subtree: accountNode(acc),
    parent: parent
      ? {
          guid: parent.guid,
          name: parent.name,
          description: parent.description,
          balance: nodeBalance(parent.guid)
        }
      : null
  };
}

function accountBalanceTrend(guid: string): MockPayloads['fetch_account_balance_trend'] {
  const inSubtree = subtreeGuids(guid);
  const points: { date: string; balance: number }[] = [];
  const today = new Date();
  for (let i = 11; i >= 0; i--) {
    const end = new Date(today.getFullYear(), today.getMonth() - i + 1, 0);
    const date = i === 0 ? todayIso() : isoLocal(end);
    let balance = 0;
    for (const tx of visibleTxs()) {
      if (tx.postDate > date) continue;
      for (const split of tx.splits) {
        if (inSubtree.has(split.accountGuid)) balance += splitAmount(split);
      }
    }
    points.push({ date, balance: round2(balance) });
  }
  return points;
}

// ── transactions ───────────────────────────────────────────────────────────

/** Largest-magnitude split wins ties by lowest amount — mirrors `fetch_splits_for_txns`. */
function transactionTotal(tx: FixtureTx): number {
  const amounts = tx.splits.map(splitAmount);
  if (amounts.length === 0) return 0;
  return [...amounts].sort((a, b) => Math.abs(b) - Math.abs(a) || a - b)[0];
}

function toTransactionRow(tx: FixtureTx): TransactionRow {
  return {
    guid: tx.guid,
    postDate: tx.postDate,
    description: tx.description,
    num: tx.num ?? null,
    notes: tx.notes ?? null,
    splits: tx.splits.map((s, i) => {
      const acc = account(s.accountGuid);
      return {
        guid: `${tx.guid}-${i}`,
        accountGuid: s.accountGuid,
        accountName: acc?.name ?? 'Unknown account',
        accountType: acc?.accountType ?? 'ASSET',
        amount: splitAmount(s),
        memo: s.memo ?? null
      };
    }),
    totalAmount: transactionTotal(tx)
  };
}

function filterTxs(filters: Record<string, unknown> | null | undefined): FixtureTx[] {
  let rows = visibleTxs();
  if (filters) {
    const search = filters.search ? String(filters.search).toLowerCase() : null;
    const from = filters.dateFrom ? String(filters.dateFrom) : null;
    const to = filters.dateTo ? String(filters.dateTo) : null;
    if (from) rows = rows.filter((tx) => tx.postDate >= from);
    if (to) rows = rows.filter((tx) => tx.postDate <= to);
    if (search) {
      rows = rows.filter(
        (tx) =>
          (tx.description ?? '').toLowerCase().includes(search) ||
          (tx.num ?? '').toLowerCase().includes(search) ||
          (tx.notes ?? '').toLowerCase().includes(search) ||
          tx.splits.some((s) => (account(s.accountGuid)?.name ?? '').toLowerCase().includes(search))
      );
    }
  }
  return rows;
}

function transactionsResult(args: Record<string, unknown>): MockPayloads['fetch_transactions'] {
  const pageSize = Number(args.pageSize) || 25;
  const page = Math.max(1, Number(args.page) || 1);
  const sortBy = (args.sortBy ? String(args.sortBy) : 'date') as 'date' | 'description' | 'amount';
  const sortDir = (args.sortDir === 'asc' ? 'asc' : 'desc') as 'asc' | 'desc';
  const rows = filterTxs(args.filters as Record<string, unknown> | null | undefined);

  const sorted = [...rows].sort((a, b) => {
    const dir = sortDir === 'asc' ? 1 : -1;
    if (sortBy === 'description') {
      return (a.description ?? '').localeCompare(b.description ?? '') * dir;
    }
    if (sortBy === 'amount') {
      return (transactionTotal(a) - transactionTotal(b)) * dir;
    }
    return (b.postDate.localeCompare(a.postDate) || b.guid.localeCompare(a.guid)) * dir;
  });

  const total = sorted.length;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(page, totalPages);
  return {
    transactions: sorted
      .slice((safePage - 1) * pageSize, safePage * pageSize)
      .map(toTransactionRow),
    total,
    page: safePage,
    pageSize,
    totalPages
  };
}

function heatmap(args: Record<string, unknown>): MockPayloads['fetch_transaction_heatmap'] {
  const counts = new Map<string, number>();
  for (const tx of filterTxs(args.filters as Record<string, unknown> | null | undefined)) {
    counts.set(tx.postDate, (counts.get(tx.postDate) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([date, count]) => ({ date, count }))
    .sort((a, b) => a.date.localeCompare(b.date));
}

function transactionDetail(guid: string): MockPayloads['fetch_transaction'] {
  const tx = state.transactions.find((t) => t.guid === guid);
  if (!tx) return null;
  const transaction: TransactionDetail = {
    guid: tx.guid,
    postDate: tx.postDate,
    description: tx.description,
    num: tx.num ?? null,
    notes: tx.notes ?? null,
    splits: tx.splits.map((s, i) => toSplitDetail(tx, s, i)),
    totalAmount: transactionTotal(tx),
    attachmentCount: state.attachments.filter((a) => a.txGuid === tx.guid).length
  };
  const ordered = [...visibleTxs()].sort(
    (a, b) => a.postDate.localeCompare(b.postDate) || a.guid.localeCompare(b.guid)
  );
  const index = ordered.findIndex((t) => t.guid === guid);
  return {
    transaction,
    prevGuid: index > 0 ? ordered[index - 1].guid : null,
    nextGuid: index >= 0 && index < ordered.length - 1 ? ordered[index + 1].guid : null
  };
}

function createTransaction(input: TxInputLike): string {
  if (!input?.postDate || !Array.isArray(input.splits) || input.splits.length === 0) {
    throw new Error('Validation: a transaction needs a date and at least one split');
  }
  const guid = id('tx');
  state.transactions.push({
    guid,
    postDate: input.postDate,
    description: input.description ?? '',
    num: input.num ?? null,
    notes: input.notes ?? null,
    splits: input.splits.map((split) => {
      const acc = account(split.accountGuid);
      return {
        accountGuid: split.accountGuid,
        raw: splitRaw(acc?.accountType ?? 'ASSET', split.amount),
        memo: split.memo ?? null,
        quantity: split.quantity ?? null
      };
    })
  });
  return guid;
}

function updateTransaction(args: Record<string, unknown>): void {
  const guid = String(args.guid);
  const input = args.input as TxInputLike;
  const tx = state.transactions.find((t) => t.guid === guid);
  if (!tx) throw new Error(`[mock] Unknown transaction ${guid}`);
  tx.postDate = input.postDate;
  tx.description = input.description ?? '';
  tx.num = input.num ?? null;
  tx.notes = input.notes ?? null;
  tx.splits = input.splits.map((split) => {
    const acc = account(split.accountGuid);
    return {
      accountGuid: split.accountGuid,
      raw: splitRaw(acc?.accountType ?? 'ASSET', split.amount),
      memo: split.memo ?? null,
      quantity: split.quantity ?? null
    };
  });
}

function deleteTransaction(guid: string): void {
  const index = state.transactions.findIndex((t) => t.guid === guid);
  if (index === -1) throw new Error(`[mock] Unknown transaction ${guid}`);
  state.transactions.splice(index, 1);
  state.attachments = state.attachments.filter((a) => a.txGuid !== guid);
}

// ── books / settings ───────────────────────────────────────────────────────

function bookSettings(): MockPayloads['get_book_settings'] {
  return { ...state.bookSettings };
}

// ── schedules ──────────────────────────────────────────────────────────────

function stepDate(date: string, frequency: ScheduleFrequency, mult: number): string {
  const [year, month, day] = date.split('-').map(Number);
  switch (frequency) {
    case 'daily':
      return isoLocal(new Date(year, month - 1, day + mult));
    case 'weekly':
      return isoLocal(new Date(year, month - 1, day + 7 * mult));
    case 'biweekly':
      return isoLocal(new Date(year, month - 1, day + 14 * mult));
    case 'monthly':
    case 'quarterly': {
      const months = frequency === 'quarterly' ? 3 * mult : mult;
      const total = year * 12 + (month - 1) + months;
      const nextYear = Math.floor(total / 12);
      const nextMonth = ((total % 12) + 12) % 12;
      const lastDay = new Date(nextYear, nextMonth + 1, 0).getDate();
      return isoLocal(new Date(nextYear, nextMonth, Math.min(day, lastDay)));
    }
    case 'yearly': {
      const nextYear = year + mult;
      const lastDay = new Date(nextYear, month, 0).getDate();
      return isoLocal(new Date(nextYear, month - 1, Math.min(day, lastDay)));
    }
  }
}

/** Advance a schedule so `nextDueDate` is today or later, counting posted occurrences. */
function advanceSchedule(schedule: Schedule): Schedule {
  const today = todayIso();
  let next = schedule.nextDueDate;
  let posted = schedule.occurrencesPosted;
  while (next !== null && next < today) {
    posted += 1;
    if (schedule.maxOccurrences !== null && posted >= schedule.maxOccurrences) {
      next = null;
      break;
    }
    const candidate = stepDate(next, schedule.frequency, schedule.intervalMult);
    if (schedule.endDate !== null && candidate > schedule.endDate) {
      next = null;
      break;
    }
    next = candidate;
  }
  return { ...schedule, nextDueDate: next, occurrencesPosted: posted };
}

function buildSchedule(def: FixtureScheduleDef): Schedule {
  const splits = def.splits.map((split) => ({
    accountGuid: split.accountGuid,
    accountName: account(split.accountGuid)?.name ?? null,
    amount: split.amount,
    memo: split.memo ?? null
  }));
  const previewAmount = splits.reduce(
    (sum, split) => (split.amount > 0 ? sum + split.amount : sum),
    0
  );
  return advanceSchedule({
    guid: def.guid,
    name: def.name,
    frequency: def.frequency,
    intervalMult: def.intervalMult,
    startDate: def.startDate,
    endDate: def.endDate,
    maxOccurrences: def.maxOccurrences,
    occurrencesPosted: 0,
    nextDueDate: def.startDate,
    active: def.active,
    description: def.description,
    num: def.num,
    notes: def.notes,
    splits,
    previewAmount,
    createdAt: '2026-04-01T00:00:00Z'
  });
}

state.schedules = SCHEDULES.map(buildSchedule);

/** Post one occurrence of a schedule and advance the pointer. */
function postScheduleOccurrence(guid: string): string {
  const schedule = state.schedules.find((s) => s.guid === guid);
  if (!schedule || schedule.nextDueDate === null) {
    throw new Error(`[mock] Schedule ${guid} has no due occurrence`);
  }
  const txGuid = createTransaction({
    postDate: schedule.nextDueDate,
    description: schedule.name,
    num: schedule.num ?? null,
    notes: schedule.notes ?? null,
    splits: schedule.splits.map((split) => ({
      accountGuid: split.accountGuid,
      amount: split.amount,
      memo: split.memo ?? null
    }))
  });
  const advanced = advanceSchedule({
    ...schedule,
    nextDueDate: stepDate(schedule.nextDueDate, schedule.frequency, schedule.intervalMult)
  });
  Object.assign(schedule, advanced);
  return txGuid;
}

function skipScheduleOccurrence(guid: string): string | null {
  const schedule = state.schedules.find((s) => s.guid === guid);
  if (!schedule || schedule.nextDueDate === null) {
    throw new Error(`[mock] Schedule ${guid} has no due occurrence`);
  }
  Object.assign(
    schedule,
    advanceSchedule({
      ...schedule,
      nextDueDate: stepDate(schedule.nextDueDate, schedule.frequency, schedule.intervalMult)
    })
  );
  return schedule.nextDueDate;
}

// ── budgets ────────────────────────────────────────────────────────────────

function budgetFor(accountGuid: string, year: number) {
  return state.budgets.find((b) => b.accountGuid === accountGuid && b.year === year) ?? null;
}

function actualFor(accountGuid: string, year: number, month?: number): number {
  const inSubtree = subtreeGuids(accountGuid);
  const prefix = month === undefined ? `${year}-` : `${year}-${String(month).padStart(2, '0')}`;
  let sum = 0;
  for (const tx of visibleTxs()) {
    if (!tx.postDate.startsWith(prefix)) continue;
    for (const split of tx.splits) {
      if (inSubtree.has(split.accountGuid)) sum += splitAmount(split);
    }
  }
  return sum;
}

const elapsedMonths = (year: number): number => {
  const now = new Date();
  if (year < now.getFullYear()) return 12;
  if (year > now.getFullYear()) return 0;
  return now.getMonth() + 1;
};

function budgetActualRows(year: number): MockPayloads['fetch_budget_vs_actual'] {
  const elapsed = elapsedMonths(year);
  return state.budgets
    .filter((b) => b.year === year)
    .map((b) => {
      const acc = account(b.accountGuid);
      const actual = actualFor(b.accountGuid, year);
      const expectedYtd = (b.amount * elapsed) / 12;
      return {
        guid: `budget-${b.accountGuid}-${year}`,
        accountGuid: b.accountGuid,
        accountName: acc?.name ?? b.accountGuid,
        accountDescription: acc?.description ?? undefined,
        accountType: acc?.accountType ?? 'EXPENSE',
        budgetAmount: b.amount,
        actualAmount: round2(actual),
        remaining: round2(b.amount - actual),
        percentage: b.amount ? round2((actual / b.amount) * 100) : 0,
        rolloverBalance: 0,
        expectedYtd: round2(expectedYtd),
        pacingPercentage: expectedYtd ? round2((actual / expectedYtd) * 100) : 0,
        delta: round2(expectedYtd - actual),
        elapsedMonths: elapsed,
        effectiveTotal: b.amount
      };
    });
}

function budgetDetail(accountGuid: string, year: number): MockPayloads['fetch_budget_detail'] {
  const acc = account(accountGuid);
  const plan = budgetFor(accountGuid, year);
  if (!acc || !plan) throw new Error(`[mock] No budget for ${accountGuid} in ${year}`);
  const elapsed = elapsedMonths(year);
  const actual = actualFor(accountGuid, year);
  const monthly = Array.from({ length: 12 }, (_, i) => {
    const monthActual = actualFor(accountGuid, year, i + 1);
    return {
      month: i + 1,
      actualAmount: round2(monthActual)
    };
  });
  let cumulativeActual = 0;
  const monthlyEntries = monthly.map((entry, i) => {
    cumulativeActual += entry.actualAmount;
    const cumulativeExpected = (plan.amount * (i + 1)) / 12;
    return {
      month: entry.month,
      actualAmount: entry.actualAmount,
      cumulativeActual: round2(cumulativeActual),
      cumulativeExpected: round2(cumulativeExpected),
      delta: round2(cumulativeExpected - cumulativeActual),
      pacingPercentage: cumulativeExpected
        ? round2((cumulativeActual / cumulativeExpected) * 100)
        : 0
    };
  });
  const expectedYtd = (plan.amount * elapsed) / 12;
  return {
    budget: {
      guid: `budget-${accountGuid}-${year}`,
      accountGuid,
      accountName: acc.name,
      accountDescription: acc.description ?? undefined,
      accountType: acc.accountType,
      year,
      amount: plan.amount,
      createdAt: '2026-01-01T00:00:00Z'
    },
    summary: {
      budgetAmount: plan.amount,
      rolloverBalance: 0,
      effectiveTotal: plan.amount,
      actualAmount: round2(actual),
      expectedYtd: round2(expectedYtd),
      remaining: round2(plan.amount - actual),
      percentage: plan.amount ? round2((actual / plan.amount) * 100) : 0,
      pacingPercentage: expectedYtd ? round2((actual / expectedYtd) * 100) : 0,
      delta: round2(expectedYtd - actual),
      elapsedMonths: elapsed
    },
    monthlyEntries
  };
}

// ── portfolio ──────────────────────────────────────────────────────────────

function valueTrend(
  cost: number,
  marketValue: number,
  weeks = 16
): { date: string; marketValue: number; costBasis: number }[] {
  const points: { date: string; marketValue: number; costBasis: number }[] = [];
  for (let i = weeks - 1; i >= 0; i--) {
    const date = new Date();
    date.setDate(date.getDate() - i * 7);
    const t = (weeks - 1 - i) / (weeks - 1);
    const wiggle = Math.sin((i + cost) * 1.7) * (marketValue - cost) * 0.08;
    points.push({
      date: isoLocal(date),
      marketValue: round2(cost + (marketValue - cost) * t + wiggle),
      costBasis: round2(cost)
    });
  }
  return points;
}

function holdingFor(acc: FixtureAccount) {
  const { shares, cost } = investmentTotals(acc.guid);
  const price = marketPrice(acc.commodity);
  const marketValue = price === null ? cost : shares * price;
  const market = MARKET_PRICES[acc.commodity ?? ''];
  return {
    commodityGuid: acc.commodityGuid ?? acc.guid,
    mnemonic: acc.commodity ?? acc.name,
    fullName: COMMODITIES.find((c) => c.guid === acc.commodityGuid)?.fullName ?? null,
    namespace: COMMODITIES.find((c) => c.guid === acc.commodityGuid)?.namespace ?? 'NASDAQ',
    shares: round2(shares),
    costBasis: round2(cost),
    marketValue: round2(marketValue),
    avgCostPerShare: shares ? round2(cost / shares) : 0,
    gainLoss: round2(marketValue - cost),
    gainLossPct: cost ? round2(((marketValue - cost) / cost) * 100) : 0,
    allocationPct: 0,
    price,
    prevPrice: market?.prevPrice ?? null,
    priceDate: market?.priceDate ?? null,
    accounts: [
      {
        accountGuid: acc.guid,
        accountName: acc.name,
        accountDescription: acc.description,
        shares: round2(shares),
        costBasis: round2(cost)
      }
    ]
  };
}

function investmentAccounts(): FixtureAccount[] {
  return state.accounts.filter((a) => INVESTMENT_TYPES.has(a.accountType));
}

function portfolioData(): MockPayloads['fetch_portfolio_data'] {
  const holdings = investmentAccounts().map(holdingFor);
  const totalMarketValue = holdings.reduce((sum, h) => sum + h.marketValue, 0);
  const totalCostBasis = holdings.reduce((sum, h) => sum + h.costBasis, 0);
  for (const holding of holdings) {
    holding.allocationPct = totalMarketValue
      ? round2((holding.marketValue / totalMarketValue) * 100)
      : 0;
  }
  return {
    totalMarketValue: round2(totalMarketValue),
    totalCostBasis: round2(totalCostBasis),
    totalGainLoss: round2(totalMarketValue - totalCostBasis),
    totalGainLossPct: totalCostBasis
      ? round2(((totalMarketValue - totalCostBasis) / totalCostBasis) * 100)
      : 0,
    lastRefreshed: nowIso(),
    holdings,
    valueTrend: valueTrend(totalCostBasis, totalMarketValue)
  };
}

function allocationTargets(): MockPayloads['fetch_allocation_targets'] {
  return [...state.allocationTargets.entries()]
    .map(([commodityGuid, targetPct]) => ({ commodityGuid, targetPct }))
    .sort((a, b) => {
      const ma = COMMODITIES.find((c) => c.guid === a.commodityGuid)?.mnemonic ?? '';
      const mb = COMMODITIES.find((c) => c.guid === b.commodityGuid)?.mnemonic ?? '';
      return ma.localeCompare(mb) || a.commodityGuid.localeCompare(b.commodityGuid);
    });
}

function holdingDetail(args: Record<string, unknown>): MockPayloads['fetch_holding_detail'] {
  const commodityGuid = String(args.commodityGuid);
  const acc = investmentAccounts().find((a) => a.commodityGuid === commodityGuid);
  if (!acc) throw new Error(`[mock] Unknown holding ${commodityGuid}`);
  const holding = holdingFor(acc);
  const pageSize = Number(args.pageSize) || 25;
  const page = Math.max(1, Number(args.page) || 1);

  const history: { date: string; price: number; currency: string }[] = [];
  const base = holding.prevPrice ?? holding.price ?? 1;
  const last = holding.price ?? base;
  for (let i = 29; i >= 0; i--) {
    const date = new Date();
    date.setDate(date.getDate() - i);
    const t = (29 - i) / 29;
    history.push({
      date: isoLocal(date),
      price: round2(base + (last - base) * t + Math.sin(i * 1.3) * last * 0.004),
      currency: state.baseCurrency
    });
  }

  const txs = visibleTxs().filter((tx) => tx.splits.some((s) => s.accountGuid === acc.guid));
  const ascending = [...txs].sort((a, b) => a.postDate.localeCompare(b.postDate));
  let runningShares = 0;
  const rows = ascending.map((tx) => {
    const relevant = tx.splits.filter((s) => s.accountGuid === acc.guid);
    const sharesDelta = relevant.reduce((sum, s) => sum + (s.quantity ?? 0), 0);
    runningShares += sharesDelta;
    return {
      guid: tx.guid,
      postDate: tx.postDate,
      description: tx.description,
      num: tx.num ?? null,
      notes: tx.notes ?? null,
      accountName: acc.name,
      accountDescription: acc.description,
      sharesDelta: round2(sharesDelta),
      valueDelta: round2(relevant.reduce((sum, s) => sum + splitAmount(s), 0)),
      runningShares: round2(runningShares)
    };
  });
  const displayed = [...rows].reverse();
  const total = displayed.length;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(page, totalPages);
  return {
    holding,
    priceHistory: history,
    valueTrend: valueTrend(holding.costBasis, holding.marketValue),
    transactions: displayed.slice((safePage - 1) * pageSize, safePage * pageSize),
    total,
    page: safePage,
    pageSize,
    totalPages
  };
}

// ── reports ────────────────────────────────────────────────────────────────

function periodTotals(from: string, to: string, category: string): Map<string, number> {
  const guids = new Set(
    state.accounts.filter((a) => categoryOf(a.accountType) === category).map((a) => a.guid)
  );
  const totals = new Map<string, number>();
  for (const tx of visibleTxs()) {
    if (tx.postDate < from || tx.postDate > to) continue;
    for (const split of tx.splits) {
      if (guids.has(split.accountGuid)) {
        totals.set(split.accountGuid, (totals.get(split.accountGuid) ?? 0) + splitAmount(split));
      }
    }
  }
  return totals;
}

function pnlRows(
  category: 'INCOME' | 'EXPENSE',
  from: string,
  to: string,
  prevFrom: string,
  prevTo: string
) {
  const current = periodTotals(from, to, category);
  const previous = periodTotals(prevFrom, prevTo, category);
  const guids = new Set([...current.keys(), ...previous.keys()]);
  const rows = state.accounts
    .filter((a) => guids.has(a.guid))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((a) => {
      const currentAmount = round2(current.get(a.guid) ?? 0);
      const previousAmount = round2(previous.get(a.guid) ?? 0);
      return {
        accountGuid: a.guid,
        accountName: a.name,
        accountDescription: a.description ?? undefined,
        indent: 0,
        currentAmount,
        previousAmount,
        changePct: previousAmount
          ? round2(((currentAmount - previousAmount) / Math.abs(previousAmount)) * 100)
          : null
      };
    });
  return rows;
}

function pnlReport(args: Record<string, unknown>): MockPayloads['fetch_pnl_report'] {
  const dateFrom = String(args.dateFrom);
  const dateTo = String(args.dateTo);
  const prevDateFrom = args.prevDateFrom ? String(args.prevDateFrom) : dateFrom;
  const prevDateTo = args.prevDateTo ? String(args.prevDateTo) : dateTo;
  const incomeRows = pnlRows('INCOME', dateFrom, dateTo, prevDateFrom, prevDateTo);
  const expenseRows = pnlRows('EXPENSE', dateFrom, dateTo, prevDateFrom, prevDateTo);
  const sum = (rows: { currentAmount: number }[]) =>
    round2(rows.reduce((total, row) => total + row.currentAmount, 0));
  const sumPrev = (rows: { previousAmount: number }[]) =>
    round2(rows.reduce((total, row) => total + row.previousAmount, 0));
  const totalIncome = sum(incomeRows);
  const totalIncomePrev = sumPrev(incomeRows);
  const totalExpenses = sum(expenseRows);
  const totalExpensesPrev = sumPrev(expenseRows);
  const netIncome = round2(totalIncome - totalExpenses);
  const netIncomePrev = round2(totalIncomePrev - totalExpensesPrev);
  return {
    dateFrom,
    dateTo,
    prevDateFrom,
    prevDateTo,
    reportCurrency: state.baseCurrency,
    incomeRows,
    incomeRootGuid: 'income',
    totalIncome,
    totalIncomePrev,
    expenseRows,
    expenseRootGuid: 'expenses',
    totalExpenses,
    totalExpensesPrev,
    netIncome,
    netIncomePrev,
    netIncomeChangePct: netIncomePrev
      ? round2(((netIncome - netIncomePrev) / Math.abs(netIncomePrev)) * 100)
      : null,
    missingFx: []
  };
}

function pnlTrend(args: Record<string, unknown>): MockPayloads['fetch_pnl_trend'] {
  const dateFrom = String(args.dateFrom);
  const dateTo = String(args.dateTo);
  const months: string[] = [];
  const cursor = new Date(`${dateFrom.slice(0, 7)}-01T00:00:00`);
  const end = dateTo.slice(0, 7);
  while (isoLocal(cursor).slice(0, 7) <= end) {
    months.push(isoLocal(cursor).slice(0, 7));
    cursor.setMonth(cursor.getMonth() + 1);
  }
  const income = periodTotals(dateFrom, dateTo, 'INCOME');
  const expenses = periodTotals(dateFrom, dateTo, 'EXPENSE');
  return months.map((month) => {
    const monthIncome = [...income.keys()].reduce((sum, guid) => {
      const subtree = subtreeGuids(guid);
      let total = 0;
      for (const tx of visibleTxs()) {
        if (!tx.postDate.startsWith(month)) continue;
        for (const split of tx.splits) {
          if (subtree.has(split.accountGuid)) total += splitAmount(split);
        }
      }
      return sum + total;
    }, 0);
    const monthExpenses = [...expenses.keys()].reduce((sum, guid) => {
      const subtree = subtreeGuids(guid);
      let total = 0;
      for (const tx of visibleTxs()) {
        if (!tx.postDate.startsWith(month)) continue;
        for (const split of tx.splits) {
          if (subtree.has(split.accountGuid)) total += splitAmount(split);
        }
      }
      return sum + total;
    }, 0);
    return { month, income: round2(monthIncome), expenses: round2(monthExpenses) };
  });
}

// ── reconcile ──────────────────────────────────────────────────────────────

function reconcileState(split: FixtureSplit): string {
  return split.reconcileState ?? 'n';
}

function reconcileInfo(args: Record<string, unknown>): MockPayloads['fetch_reconcile_info'] {
  const accountGuid = String(args.accountGuid);
  const statementDate = String(args.statementDate);
  const acc = account(accountGuid);
  if (!acc) throw new Error(`[mock] Unknown account ${accountGuid}`);
  const inSubtree = subtreeGuids(accountGuid);
  let startingBalance = 0;
  const candidates: MockPayloads['fetch_reconcile_info']['candidates'] = [];
  for (const tx of visibleTxs()) {
    if (tx.postDate > statementDate) continue;
    tx.splits.forEach((split, index) => {
      if (!inSubtree.has(split.accountGuid)) return;
      if (reconcileState(split) === 'y') {
        startingBalance += splitAmount(split);
      } else {
        candidates.push({
          splitGuid: `${tx.guid}-${index}`,
          txGuid: tx.guid,
          postDate: tx.postDate,
          description: tx.description,
          num: tx.num ?? null,
          memo: split.memo ?? null,
          amount: splitAmount(split),
          reconcileState: reconcileState(split)
        });
      }
    });
  }
  candidates.sort((a, b) => a.postDate.localeCompare(b.postDate));
  return {
    accountGuid,
    accountName: acc.name,
    accountType: acc.accountType,
    commodity: acc.commodity,
    statementDate,
    startingBalance: round2(startingBalance),
    candidates
  };
}

// ── attachments ────────────────────────────────────────────────────────────

const mimeFor = (filename: string): string => {
  const ext = filename.split('.').pop()?.toLowerCase() ?? '';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(ext))
    return `image/${ext === 'jpg' ? 'jpeg' : ext}`;
  if (ext === 'pdf') return 'application/pdf';
  return 'text/plain';
};

// ── seed reconcile states ──────────────────────────────────────────────────

const RECONCILE_TYPES = new Set(['ASSET', 'BANK', 'CASH', 'CREDIT', 'LIABILITY', 'PAYABLE']);

function daysAgo(date: string): number {
  const then = new Date(`${date}T00:00:00`);
  return Math.floor((Date.now() - then.getTime()) / 86_400_000);
}

// Recent cash-account splits stay unreconciled so the reconcile workflow has
// candidates; older ones read as cleared/reconciled.
for (const tx of state.transactions) {
  for (const split of tx.splits) {
    const acc = account(split.accountGuid);
    if (!acc || !RECONCILE_TYPES.has(acc.accountType)) continue;
    const age = daysAgo(tx.postDate);
    split.reconcileState = age <= 30 ? 'n' : age <= 60 ? 'c' : 'y';
  }
}

// ── handlers ───────────────────────────────────────────────────────────────

type HandlerMap = {
  [K in keyof MockPayloads]: (args: Record<string, unknown>) => MockPayloads[K];
};

const handlers: HandlerMap = {
  // Shell / books
  fetch_home_data: () => homeData(),
  fetch_active_book: () => ({ path: state.bookPath }),
  fetch_nav_roots: () =>
    ['ASSET', 'LIABILITY', 'EQUITY', 'INCOME', 'EXPENSE'].map((category) => {
      const root = state.accounts.find(
        (a) => a.parentGuid === 'root' && categoryOf(a.accountType) === category
      );
      return { category, guid: root?.guid ?? null, name: root?.name ?? null };
    }),
  fetch_nav_badges: () => ({
    dueSchedules: state.schedules.filter(
      (s) => s.active && s.nextDueDate !== null && s.nextDueDate <= todayIso()
    ).length,
    unclearedTransactions: visibleTxs().filter((tx) =>
      tx.splits.some(
        (s) =>
          reconcileState(s) === 'n' &&
          categoryOf(account(s.accountGuid)?.accountType ?? '') === 'ASSET'
      )
    ).length
  }),
  clear_app_db: () => {
    state.transactions = [];
    state.accounts = state.accounts.filter((a) => a.guid === 'root' || a.parentGuid === 'root');
  },

  // Accounts
  fetch_accounts: () => accountsResult(),
  fetch_account_detail: (args) => accountDetail(args),
  fetch_account_balance_trend: (args) => accountBalanceTrend(String(args.guid)),
  create_account: (args) => {
    const input = args.input as AccountInputLike;
    const guid = id('acct');
    const commodity = COMMODITIES.find((c) => c.guid === input.commodityGuid);
    state.accounts.push({
      guid,
      parentGuid: input.parentGuid ?? null,
      name: input.name,
      accountType: input.accountType,
      code: input.code ?? null,
      description: input.description ?? null,
      placeholder: input.placeholder,
      commodity: commodity?.mnemonic ?? null,
      commodityGuid: commodity?.guid ?? null
    });
    return guid;
  },
  update_account: (args) => {
    const guid = String(args.guid);
    const input = args.input as AccountInputLike;
    const acc = account(guid);
    if (!acc) throw new Error(`[mock] Unknown account ${guid}`);
    const commodity = COMMODITIES.find((c) => c.guid === input.commodityGuid);
    Object.assign(acc, {
      name: input.name,
      accountType: input.accountType,
      parentGuid: input.parentGuid ?? acc.parentGuid,
      code: input.code ?? null,
      description: input.description ?? null,
      placeholder: input.placeholder,
      commodity: commodity?.mnemonic ?? null,
      commodityGuid: commodity?.guid ?? null
    });
  },
  delete_account: (args) => {
    const guid = String(args.guid);
    const acc = account(guid);
    if (!acc) throw new Error(`[mock] Unknown account ${guid}`);
    const removed = subtreeGuids(guid);
    state.accounts = state.accounts.filter((a) => !removed.has(a.guid));
    state.transactions = state.transactions.filter(
      (tx) => !tx.splits.some((s) => removed.has(s.accountGuid))
    );
  },

  // Transactions
  fetch_transactions: (args) => transactionsResult(args),
  fetch_transaction_heatmap: (args) => heatmap(args),
  fetch_transaction: (args) => transactionDetail(String(args.guid)),
  create_transaction: (args) => createTransaction(args.input as TxInputLike),
  update_transaction: (args) => updateTransaction(args),
  delete_transaction: (args) => deleteTransaction(String(args.guid)),

  // Books
  open_book: () => emptyBookResult(),
  create_book_at: (args) => {
    state.bookPath = String(args.path);
    return emptyBookResult();
  },
  save_book_as: (args) => {
    state.bookPath = String(args.path);
    return emptyBookResult();
  },
  close_book: () => {
    state.bookPath = null;
    return { hasData: false, summary: dataSummary() };
  },
  set_base_currency: (args) => {
    state.baseCurrency = String(args.currency);
    return emptyBookResult();
  },

  // Book settings
  get_book_settings: () => bookSettings(),
  set_book_setting: (args) => {
    const key = String(args.key);
    const value = String(args.value);
    switch (key) {
      case 'fiscalYearStartMonth':
        state.bookSettings.fiscalYearStartMonth = Number(value) || 1;
        break;
      case 'defaultPnlPeriod':
        state.bookSettings.defaultPnlPeriod = value as typeof state.bookSettings.defaultPnlPeriod;
        break;
      case 'priceAutoRefresh':
        state.bookSettings.priceAutoRefresh = value as typeof state.bookSettings.priceAutoRefresh;
        break;
      case 'exportCsvDelimiter':
        state.bookSettings.exportCsvDelimiter =
          value as typeof state.bookSettings.exportCsvDelimiter;
        break;
      case 'exportDateFormat':
        state.bookSettings.exportDateFormat = value as typeof state.bookSettings.exportDateFormat;
        break;
    }
    return bookSettings();
  },
  fetch_book_currencies: () =>
    COMMODITIES.filter((c) => c.namespace === 'CURRENCY').map((c) => ({
      guid: c.guid,
      mnemonic: c.mnemonic
    })),

  // Schedules
  fetch_schedules: () => state.schedules,
  fetch_schedule: (args) => {
    const schedule = state.schedules.find((s) => s.guid === args.guid);
    if (!schedule) throw new Error(`[mock] Unknown schedule ${args.guid}`);
    return schedule;
  },
  create_schedule: (args) => {
    const input = args.input as ScheduleInputLike;
    const schedule = buildSchedule({
      guid: id('sched'),
      name: input.name,
      frequency: input.frequency,
      intervalMult: input.intervalMult ?? 1,
      startDate: input.startDate,
      endDate: input.endDate ?? null,
      maxOccurrences: input.maxOccurrences ?? null,
      description: input.description ?? null,
      num: input.num ?? null,
      notes: input.notes ?? null,
      active: true,
      splits: input.splits
    });
    state.schedules.push(schedule);
    return schedule;
  },
  update_schedule: (args) => {
    const schedule = state.schedules.find((s) => s.guid === args.guid);
    if (!schedule) throw new Error(`[mock] Unknown schedule ${args.guid}`);
    const input = args.input as ScheduleInputLike;
    const updated = buildSchedule({
      guid: schedule.guid,
      name: input.name,
      frequency: input.frequency,
      intervalMult: input.intervalMult ?? 1,
      startDate: input.startDate,
      endDate: input.endDate ?? null,
      maxOccurrences: input.maxOccurrences ?? null,
      description: input.description ?? null,
      num: input.num ?? null,
      notes: input.notes ?? null,
      active: input.active ?? schedule.active,
      splits: input.splits
    });
    Object.assign(schedule, updated);
    return schedule;
  },
  delete_schedule: (args) => {
    state.schedules = state.schedules.filter((s) => s.guid !== args.guid);
  },
  set_schedule_active: (args) => {
    const schedule = state.schedules.find((s) => s.guid === args.guid);
    if (!schedule) throw new Error(`[mock] Unknown schedule ${args.guid}`);
    schedule.active = Boolean(args.active);
    return schedule;
  },
  post_schedule_occurrence: (args) => postScheduleOccurrence(String(args.guid)),
  skip_schedule_occurrence: (args) => skipScheduleOccurrence(String(args.guid)),

  // Budgets
  fetch_budget_years: (args) => {
    const years = state.budgets
      .filter((b) => b.accountGuid === args.accountGuid)
      .map((b) => b.year);
    return years.length ? [...new Set(years)].sort() : [new Date().getFullYear()];
  },
  fetch_budget_vs_actual: (args) => budgetActualRows(Number(args.year)),
  fetch_budget_detail: (args) => budgetDetail(String(args.accountGuid), Number(args.year)),
  fetch_budget_date_range: () => {
    const dates = visibleTxs()
      .map((tx) => tx.postDate)
      .sort();
    return { earliestDate: dates[0] ?? null, latestDate: dates.at(-1) ?? null };
  },
  set_budget_year_amount: (args) => {
    const accountGuid = String(args.accountGuid);
    const year = Number(args.year);
    const amount = Number(args.amount);
    const existing = budgetFor(accountGuid, year);
    if (existing) existing.amount = amount;
    else state.budgets.push({ accountGuid, year, amount });
    const acc = account(accountGuid);
    return {
      guid: `budget-${accountGuid}-${year}`,
      accountGuid,
      accountName: acc?.name ?? accountGuid,
      accountDescription: acc?.description ?? undefined,
      accountType: acc?.accountType ?? 'EXPENSE',
      year,
      amount,
      createdAt: nowIso()
    };
  },
  delete_budget_year: (args) => {
    state.budgets = state.budgets.filter(
      (b) => !(b.accountGuid === args.accountGuid && b.year === Number(args.year))
    );
  },

  // Portfolio
  fetch_portfolio_data: () => portfolioData(),
  fetch_holding_detail: (args) => holdingDetail(args),
  fetch_allocation_targets: () => allocationTargets(),
  save_allocation_targets: (args) => {
    const targets = (args.targets ?? []) as MockPayloads['fetch_allocation_targets'];
    state.allocationTargets = new Map(targets.map((t) => [t.commodityGuid, Number(t.targetPct)]));
    return allocationTargets();
  },
  fetch_symbol_overrides: () =>
    COMMODITIES.filter((c) => c.namespace !== 'CURRENCY').map((c) => ({
      commodityGuid: c.guid,
      mnemonic: c.mnemonic,
      namespace: c.namespace,
      fullName: c.fullName,
      symbol: state.overrides.get(c.guid) ?? null
    })),
  set_symbol_override: (args) => {
    const commodityGuid = String(args.commodityGuid);
    state.overrides.set(commodityGuid, String(args.symbol));
    const commodity = COMMODITIES.find((c) => c.guid === commodityGuid);
    return {
      commodityGuid,
      mnemonic: commodity?.mnemonic ?? commodityGuid,
      namespace: commodity?.namespace ?? '',
      fullName: commodity?.fullName ?? null,
      symbol: String(args.symbol)
    };
  },
  delete_symbol_override: (args) => {
    state.overrides.delete(String(args.commodityGuid));
  },
  refresh_prices: () => ({
    prices: Object.entries(MARKET_PRICES).map(([mnemonic, quote]) => ({
      mnemonic,
      price: quote.price,
      priceDate: quote.priceDate,
      currency: state.baseCurrency
    })),
    fetchedAt: nowIso(),
    errors: []
  }),

  // Reports
  fetch_pnl_report: (args) => pnlReport(args),
  fetch_pnl_trend: (args) => pnlTrend(args),

  // Reconcile
  fetch_reconcile_info: (args) => reconcileInfo(args),
  mark_reconcile_splits: (args) => {
    const input = args.input as { updates: { splitGuid: string; state: string }[] };
    let updated = 0;
    for (const change of input.updates) {
      const found = findSplit(change.splitGuid);
      if (found) {
        found.split.reconcileState = change.state;
        updated += 1;
      }
    }
    return { updated };
  },
  finish_reconciliation: (args) => {
    const input = args.input as {
      accountGuid: string;
      statementDate: string;
      endingBalance: number;
    };
    const inSubtree = subtreeGuids(input.accountGuid);
    let startingBalance = 0;
    let reconciledCount = 0;
    for (const tx of visibleTxs()) {
      if (tx.postDate > input.statementDate) continue;
      for (const split of tx.splits) {
        if (!inSubtree.has(split.accountGuid)) continue;
        if (reconcileState(split) === 'y') startingBalance += splitAmount(split);
        else if (reconcileState(split) === 'c') {
          split.reconcileState = 'y';
          reconciledCount += 1;
        }
      }
    }
    return {
      reconciledCount,
      startingBalance: round2(startingBalance),
      endingBalance: input.endingBalance
    };
  },

  // Attachments
  fetch_attachments: (args) =>
    state.attachments
      .filter((a) => a.txGuid === args.txGuid)
      .map(({ guid, filename, mimeType, byteSize, createdAt }) => ({
        guid,
        filename,
        mimeType,
        byteSize,
        createdAt
      })),
  add_attachments: (args) => {
    const input = args.input as { txGuid: string; paths: string[] };
    if (input.paths.length !== 1) {
      throw new Error('Only a single PDF attachment is allowed');
    }
    const filename = input.paths[0].split('/').pop() ?? input.paths[0];
    const ext = filename.split('.').pop()?.toLowerCase() ?? '';
    if (ext !== 'pdf') {
      throw new Error(`Unsupported file type: .${ext || 'unknown'}`);
    }
    const mimeType = mimeFor(filename);
    state.attachments = state.attachments.filter((a) => a.txGuid !== input.txGuid);
    const record: AttachmentRecord = {
      guid: id('att'),
      txGuid: input.txGuid,
      filename,
      mimeType,
      byteSize: 48_000 + filename.length * 137,
      createdAt: nowIso(),
      isImage: false
    };
    state.attachments.push(record);
    return [
      {
        guid: record.guid,
        filename: record.filename,
        mimeType: record.mimeType,
        byteSize: record.byteSize,
        createdAt: record.createdAt
      }
    ];
  },
  delete_attachment: (args) => {
    state.attachments = state.attachments.filter((a) => a.guid !== args.guid);
  },
  fetch_attachment_data: (args) => {
    const record = state.attachments.find((a) => a.guid === args.guid);
    if (!record) throw new Error(`[mock] Unknown attachment ${args.guid}`);
    return {
      filename: record.filename,
      mimeType: record.mimeType,
      dataUrl: record.isImage ? SAMPLE_IMAGE_DATA_URL : SAMPLE_TEXT_DATA_URL
    };
  },
  export_attachment: () => undefined,
  open_attachment: () => undefined,

  // CSV import
  csv_detect: (args) => {
    const headers = ['Date', 'Description', 'Amount', 'Balance'];
    const signature = `mock-${headers.join('|').toLowerCase()}`;
    return {
      encoding: 'UTF-8',
      delimiter: ',',
      hasHeader: true,
      headers,
      columnCount: 4,
      rowCount: 12,
      sampleRows: [
        ['2026-09-02', 'COFFEE SHOP', '-4.80', '1,204.10'],
        ['2026-09-03', 'PAYROLL DEPOSIT', '3,200.00', '4,404.10'],
        ['2026-09-04', 'GROCERY MARKET', '-86.42', '4,317.68']
      ],
      filename:
        String(args.path ?? 'statement.csv')
          .split('/')
          .pop() ?? 'statement.csv',
      sha256: 'mock-sha256',
      signature,
      preset: state.csvPresets.get(signature) ?? null
    };
  },
  csv_preview: (args) => {
    const input = args.input as CsvInputLike;
    const fallbackName = account(input.offsetAccountGuid)?.name ?? '';
    const row = (
      key: number,
      raw: string[],
      postDate: string | null,
      description: string,
      amount: number | null,
      valid: boolean,
      duplicate: boolean,
      error: string | null
    ) => {
      const match = state.importRules
        .sort((a, b) => a.priority - b.priority || a.id - b.id)
        .find((r) =>
          r.matchType === 'equals'
            ? description.trim().toLowerCase() === r.pattern.trim().toLowerCase()
            : description.toLowerCase().includes(r.pattern.trim().toLowerCase())
        );
      return {
        key,
        raw,
        postDate,
        description,
        amount,
        valid,
        duplicate,
        offsetAccountName: match
          ? (account(match.accountGuid)?.name ?? fallbackName)
          : fallbackName,
        ruleMatched: Boolean(match),
        error
      };
    };
    return {
      rows: [
        row(
          0,
          ['2026-09-02', 'COFFEE SHOP', '-4.80'],
          '2026-09-02',
          'COFFEE SHOP',
          -4.8,
          true,
          false,
          null
        ),
        row(
          1,
          ['2026-09-04', 'GROCERY MARKET', '-86.42'],
          '2026-09-04',
          'GROCERY MARKET',
          -86.42,
          true,
          true,
          null
        ),
        row(
          2,
          ['2026-09-07', 'BOOKSTORE', '-32.10'],
          '2026-09-07',
          'BOOKSTORE',
          -32.1,
          true,
          false,
          null
        ),
        row(
          3,
          ['not-a-date', 'MYSTERY', 'abc'],
          null,
          'MYSTERY',
          null,
          false,
          false,
          'Unparseable date'
        )
      ],
      totalRows: 4,
      validCount: 3,
      duplicateCount: 1,
      errorCount: 1,
      minDate: '2026-09-02',
      maxDate: '2026-09-07'
    };
  },
  csv_commit: (args) => {
    const input = args.input as CsvInputLike;
    const keys = input.selectedKeys ?? [];
    let created = 0;
    for (const [index, key] of keys.entries()) {
      if (key === 1) continue; // duplicate row
      createTransaction({
        postDate: `2026-09-${String(2 + index).padStart(2, '0')}`,
        description: 'CSV import',
        splits: [
          { accountGuid: input.targetAccountGuid, amount: -(10 + index) },
          { accountGuid: input.offsetAccountGuid, amount: 10 + index }
        ]
      });
      created += 1;
    }
    if (input.rememberMapping) {
      state.csvPresets.set('mock-date|description|amount|balance', {
        signature: 'mock-date|description|amount|balance',
        name: 'Date · Description · Amount',
        encoding: 'UTF-8',
        delimiter: ',',
        hasHeader: true,
        datePreference: 'dmy',
        mapping: {
          dateCol: 0,
          descCol: 1,
          amountCol: 2,
          debitCol: null,
          creditCol: null,
          numCol: null,
          notesCol: null
        },
        targetAccountGuid: input.targetAccountGuid,
        offsetAccountGuid: input.offsetAccountGuid
      });
    }
    return {
      created,
      skippedDuplicates: keys.includes(1) ? 1 : 0,
      skippedErrors: 0,
      entryId: nextId++,
      label: 'CSV import'
    };
  },
  delete_csv_preset: (args) => {
    state.csvPresets.delete(String(args.signature));
    return undefined;
  },

  // Import rules
  fetch_import_rules: () =>
    state.importRules.map((r) => ({ ...r, accountName: account(r.accountGuid)?.name ?? '' })),
  create_import_rule: (args) => {
    const input = args.input as Omit<ImportRuleLike, 'id'>;
    const rule = { id: state.nextImportRuleId++, ...input };
    state.importRules.push(rule);
    return { ...rule, accountName: account(rule.accountGuid)?.name ?? '' };
  },
  update_import_rule: (args) => {
    const input = args.input as Omit<ImportRuleLike, 'id'>;
    const rule = state.importRules.find((r) => r.id === Number(args.id));
    if (!rule) throw new Error('[mock] Unknown import rule');
    Object.assign(rule, input);
    return { ...rule, accountName: account(rule.accountGuid)?.name ?? '' };
  },
  delete_import_rule: (args) => {
    state.importRules = state.importRules.filter((r) => r.id !== Number(args.id));
    return undefined;
  },

  // Exports (written straight to disk by the backend). CSV files are no-ops in
  // the browser mock; the GnuCash export reports fixture counts so its toast
  // reads like the real one.
  export_transactions_csv: () => undefined,
  export_accounts_csv: () => undefined,
  export_gnucash: () => ({
    accounts: state.accounts.length,
    transactions: state.transactions.length,
    splits: state.transactions.reduce((total, tx) => total + tx.splits.length, 0),
    prices: Object.keys(MARKET_PRICES).length,
    budgets: state.budgets.length,
    schedules: state.schedules.length,
    futureDated: 0,
    attachmentsSkipped: state.attachments.length,
    roundedAmounts: 0
  }),

  // Global search
  search_global: (args) => {
    const query = String(args.query ?? '')
      .trim()
      .toLowerCase();
    const limit = Number(args.limitPerGroup) || 5;
    if (!query) return { accounts: [], transactions: [], holdings: [], budgets: [] };
    const accounts = state.accounts
      .filter(
        (a) =>
          a.accountType !== 'ROOT' &&
          (a.name.toLowerCase().includes(query) || accountPath(a).toLowerCase().includes(query))
      )
      .slice(0, limit)
      .map((a) => ({
        guid: a.guid,
        name: a.name,
        fullPath: accountPath(a),
        accountType: a.accountType,
        code: a.code
      }));
    const transactions = visibleTxs()
      .filter(
        (tx) =>
          (tx.description ?? '').toLowerCase().includes(query) ||
          (tx.num ?? '').toLowerCase().includes(query)
      )
      .slice(0, limit)
      .map((tx) => ({
        guid: tx.guid,
        postDate: tx.postDate,
        description: tx.description,
        num: tx.num ?? null
      }));
    const holdings = COMMODITIES.filter(
      (c) =>
        c.namespace !== 'CURRENCY' &&
        (c.mnemonic.toLowerCase().includes(query) ||
          (c.fullName ?? '').toLowerCase().includes(query))
    )
      .slice(0, limit)
      .map((c) => ({
        commodityGuid: c.guid,
        mnemonic: c.mnemonic,
        fullName: c.fullName,
        namespace: c.namespace
      }));
    const budgets = state.budgets
      .filter((b) => (account(b.accountGuid)?.name ?? '').toLowerCase().includes(query))
      .slice(0, limit)
      .map((b) => ({
        accountGuid: b.accountGuid,
        accountName: account(b.accountGuid)?.name ?? b.accountGuid,
        year: b.year
      }));
    return { accounts, transactions, holdings, budgets };
  },

  // Misc
  preview_gnucash_import: (args) => ({
    path: String(args.path ?? '/home/demo/Books/demo.gnucash'),
    filename: String(args.filename ?? 'demo.gnucash'),
    sha256: 'mock-sha256',
    accountsAdded: 12,
    accountsUpdated: 1,
    accountsOrphaned: 2,
    transactionsAdded: 0,
    transactionsUpdated: 0,
    transactionsOrphaned: 3,
    splitsAdded: 0,
    splitsUpdated: 0,
    splitsOrphaned: 7,
    commoditiesAdded: 0,
    commoditiesUpdated: 0,
    commoditiesOrphaned: 0,
    schedulesAdded: 2,
    schedulesUpdated: 0,
    orphanAccountNames: ['Local Savings', 'Holiday Fund'],
    skippedSchedules: [{ name: 'Twice a month', reason: 'more than one recurrence rule' }]
  }),
  import_gnucash: () => importResult()
};

function accountPath(acc: FixtureAccount | undefined): string {
  if (!acc) return 'Unknown';
  const parts = [acc.name];
  let parent = acc.parentGuid ? account(acc.parentGuid) : undefined;
  while (parent && parent.accountType !== 'ROOT') {
    parts.unshift(parent.name);
    parent = parent.parentGuid ? account(parent.parentGuid) : undefined;
  }
  return parts.join(':');
}

function importResult(): MockPayloads['import_gnucash'] {
  const summary = dataSummary();
  return {
    accountsAdded: 12,
    transactionsAdded: 0,
    splitsAdded: 0,
    commoditiesAdded: 0,
    accountsUpdated: 0,
    transactionsUpdated: 0,
    splitsUpdated: 0,
    accountsOrphaned: 0,
    transactionsOrphaned: 0,
    splitsOrphaned: 0,
    commoditiesOrphaned: 0,
    schedulesAdded: 2,
    schedulesUpdated: 0,
    schedulesSkipped: 1,
    totalAccounts: summary.totalAccounts,
    totalTransactions: summary.totalTransactions,
    summary,
    prices: { prices: [], fetchedAt: nowIso(), errors: [] }
  };
}

/** Commands with a deliberately inert implementation (dialogs/paths need Tauri). */

export async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const handler = (
    handlers as Record<string, ((a: Record<string, unknown>) => unknown) | undefined>
  )[cmd];
  if (!handler) {
    throw new Error(
      `[mock] No fixture for '${cmd}'. Add a handler in src/lib/dev/mock-invoke.ts ` +
        `(or run 'bun run tauri:dev' to use the real backend).`
    );
  }
  // A small delay keeps async loading states (skeletons, spinners) observable.
  await new Promise((resolve) => setTimeout(resolve, 20));
  return handler(args ?? {}) as T;
}
