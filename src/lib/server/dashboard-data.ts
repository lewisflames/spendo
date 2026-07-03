import Database from 'better-sqlite3';
import { getDbPath } from './uploaded-db';
import { getUnifiedPricesMap } from './prices';

export interface MonthBreakdown {
  month: string;
  income: number;
  expenses: number;
  net: number;
}

export interface TopCategory {
  name: string;
  total: number;
}

export interface NetWorthPoint {
  month: string;
  netWorth: number;
}

export interface DashboardTransaction {
  guid: string;
  postDate: string;
  description: string;
  amount: number;
  accountName: string;
  accountType: string;
}

export interface DashboardData {
  netWorth: number;
  netWorthTrend: NetWorthPoint[];
  monthlyBreakdown: MonthBreakdown[];
  topCategories: TopCategory[];
  recentTransactions: DashboardTransaction[];
  currentMonthIncome: number;
  currentMonthExpenses: number;
  currentMonthNet: number;
}

function formatMonth(n: number): string {
  const y = Math.floor(n / 12);
  const m = n % 12;
  return `${String(y).padStart(4, '0')}-${String(m + 1).padStart(2, '0')}`;
}

function monthNumber(ym: string): number {
  const [y, m] = ym.split('-').map(Number);
  return y * 12 + (m - 1);
}

export function getDashboardData(token: string, monthCount?: number | null): DashboardData {
  const path = getDbPath(token);
  const db = new Database(path, { readonly: true });

  try {
    const now = new Date();
    const endMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;

    const effectiveCount = monthCount === undefined ? 12 : monthCount;

    let startMonth: string;
    if (effectiveCount === null) {
      startMonth = '0000-01';
    } else {
      const startNum = Math.max(0, monthNumber(endMonth) - (effectiveCount - 1));
      startMonth = formatMonth(startNum);
    }

    const monthlyRows = db
      .prepare(
        `SELECT
					substr(t.post_date, 1, 7) as month,
					ABS(COALESCE(SUM(CASE WHEN a.account_type = 'INCOME' THEN CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) ELSE 0 END), 0)) as income,
					ABS(COALESCE(SUM(CASE WHEN a.account_type = 'EXPENSE' THEN CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) ELSE 0 END), 0)) as expenses
				FROM transactions t
				JOIN splits s ON s.tx_guid = t.guid
				JOIN accounts a ON a.guid = s.account_guid
				WHERE a.account_type IN ('INCOME', 'EXPENSE')
					AND t.post_date < date('now', '+1 day')
					AND substr(t.post_date, 1, 7) >= ?
					AND substr(t.post_date, 1, 7) <= ?
				GROUP BY month
				ORDER BY month DESC`
      )
      .all(startMonth, endMonth) as { month: string; income: number; expenses: number }[];

    const monthlyBreakdown: MonthBreakdown[] = monthlyRows.map((r) => ({
      month: r.month,
      income: r.income,
      expenses: r.expenses,
      net: r.income - r.expenses
    }));

    // Net worth — assets minus liabilities (always unfiltered)
    // Non-investment assets + all liabilities use value_num/value_denom (cost basis)
    const baseRow = db
      .prepare(
        `SELECT
					COALESCE(SUM(CASE WHEN a.account_type IN ('ASSET','BANK','CASH','CURRENCY','RECEIVABLE') THEN CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) ELSE 0 END), 0) as assets,
					COALESCE(SUM(CASE WHEN a.account_type IN ('LIABILITY','CREDIT','PAYABLE') THEN CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) ELSE 0 END), 0) as liabilities
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE t.post_date < date('now', '+1 day')`
      )
      .get() as { assets: number; liabilities: number };

    // Investment accounts: quantity (shares) × current market price, or cost basis as fallback
    const stockRows = db
      .prepare(
        `SELECT c.mnemonic,
					SUM(CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)) as shares,
					SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) as cost_basis
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN commodities c ON c.guid = a.commodity_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE a.account_type IN ('STOCK', 'MUTUAL')
					AND t.post_date < date('now', '+1 day')
				GROUP BY c.mnemonic`
      )
      .all() as { mnemonic: string; shares: number; cost_basis: number }[];

    const unifiedPrices = getUnifiedPricesMap(token);
    let investmentValue = 0;
    for (const row of stockRows) {
      const price = unifiedPrices?.get(row.mnemonic);
      if (price) {
        investmentValue += row.shares * price;
      } else {
        investmentValue += row.cost_basis;
      }
    }

    const netWorth = baseRow.assets + investmentValue + baseRow.liabilities;

    // Net worth trend — baseline before the window + monthly deltas
    let trendStartMonth: string;
    if (effectiveCount === null) {
      trendStartMonth = '0000-01';
    } else {
      trendStartMonth = formatMonth(Math.max(0, monthNumber(endMonth) - (effectiveCount - 1)));
    }

    const baselineRow = db
      .prepare(
        `SELECT COALESCE(SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)), 0) as baseline
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE a.account_type IN ('ASSET','BANK','CASH','LIABILITY','CREDIT')
					AND t.post_date < ?`
      )
      .get(trendStartMonth) as { baseline: number };

    const deltaRows = db
      .prepare(
        `SELECT
					substr(t.post_date, 1, 7) as month,
					SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) as delta
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE a.account_type IN ('ASSET','BANK','CASH','LIABILITY','CREDIT')
					AND t.post_date >= ?
					AND t.post_date < date('now', '+1 day')
				GROUP BY month
				ORDER BY month ASC`
      )
      .all(trendStartMonth) as { month: string; delta: number }[];

    // Investment share quantity tracking — baseline shares before window
    const baselineShareRows = db
      .prepare(
        `SELECT c.mnemonic,
					SUM(CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)) as shares
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN commodities c ON c.guid = a.commodity_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE a.account_type IN ('STOCK', 'MUTUAL')
					AND t.post_date < ?
				GROUP BY c.mnemonic`
      )
      .all(trendStartMonth) as { mnemonic: string; shares: number }[];

    // Monthly share deltas within the window
    const shareDeltaRows = db
      .prepare(
        `SELECT
					substr(t.post_date, 1, 7) as month,
					c.mnemonic,
					SUM(CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)) as share_delta
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN commodities c ON c.guid = a.commodity_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE a.account_type IN ('STOCK', 'MUTUAL')
					AND t.post_date >= ?
					AND t.post_date < date('now', '+1 day')
				GROUP BY month, c.mnemonic
				ORDER BY month ASC`
      )
      .all(trendStartMonth) as { month: string; mnemonic: string; share_delta: number }[];

    // Build trend — non-investment cost-basis + investment market value
    const netWorthTrend: NetWorthPoint[] = [];
    let nonInvRunning = baselineRow.baseline;
    const nonInvDeltaMap = new Map(deltaRows.map((r) => [r.month, r.delta]));

    const sharesRunning = new Map<string, number>();
    for (const row of baselineShareRows) {
      sharesRunning.set(row.mnemonic, row.shares);
    }

    const shareDeltaByMonth = new Map<string, Map<string, number>>();
    for (const row of shareDeltaRows) {
      let monthMap = shareDeltaByMonth.get(row.month);
      if (!monthMap) {
        monthMap = new Map();
        shareDeltaByMonth.set(row.month, monthMap);
      }
      monthMap.set(row.mnemonic, row.share_delta);
    }

    function applyShareDeltas(month: string) {
      const deltas = shareDeltaByMonth.get(month);
      if (deltas) {
        for (const [mnemonic, delta] of deltas) {
          sharesRunning.set(mnemonic, (sharesRunning.get(mnemonic) ?? 0) + delta);
        }
      }
    }

    function currentInvestmentValue(): number {
      let v = 0;
      for (const [mnemonic, shares] of sharesRunning) {
        const price = unifiedPrices?.get(mnemonic) ?? 1;
        v += shares * price;
      }
      return v;
    }

    if (effectiveCount === null) {
      const allMonths = [...nonInvDeltaMap.keys(), ...shareDeltaByMonth.keys()];
      if (allMonths.length === 0) {
        netWorthTrend.push({
          month: endMonth,
          netWorth: nonInvRunning + currentInvestmentValue()
        });
      } else {
        allMonths.sort();
        let m = allMonths[0];
        while (monthNumber(m) <= monthNumber(endMonth)) {
          if (nonInvDeltaMap.has(m)) {
            nonInvRunning += nonInvDeltaMap.get(m)!;
          }
          applyShareDeltas(m);
          netWorthTrend.push({
            month: m,
            netWorth: nonInvRunning + currentInvestmentValue()
          });
          m = formatMonth(monthNumber(m) + 1);
        }
      }
    } else {
      for (let i = 0; i < effectiveCount; i++) {
        const m = formatMonth(monthNumber(trendStartMonth) + i);
        if (nonInvDeltaMap.has(m)) {
          nonInvRunning += nonInvDeltaMap.get(m)!;
        }
        applyShareDeltas(m);
        netWorthTrend.push({
          month: m,
          netWorth: nonInvRunning + currentInvestmentValue()
        });
      }
    }

    // Period totals for KPI cards (from monthly breakdown)
    const periodIncome = monthlyBreakdown.reduce((sum, r) => sum + r.income, 0);
    const periodExpenses = monthlyBreakdown.reduce((sum, r) => sum + r.expenses, 0);

    // Top spending categories — use ABS(SUM(...)) so contra-entries
    // properly reduce the category total before the absolute is taken.
    const allCategories = db
      .prepare(
        `SELECT a.name, ABS(SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL))) as total
				FROM splits s
				JOIN accounts a ON a.guid = s.account_guid
				JOIN transactions t ON t.guid = s.tx_guid
				WHERE a.account_type = 'EXPENSE'
					AND t.post_date < date('now', '+1 day')
					AND t.post_date >= ?
				GROUP BY a.name
				ORDER BY total DESC`
      )
      .all(startMonth) as TopCategory[];

    let topCategories: TopCategory[];
    if (allCategories.length <= 9) {
      topCategories = allCategories;
    } else {
      topCategories = allCategories.slice(0, 9);
      const otherTotal = allCategories.slice(9).reduce((sum, c) => sum + c.total, 0);
      if (otherTotal > 0) {
        topCategories.push({ name: 'Other', total: otherTotal });
      }
    }

    // Recent transactions (last 10)
    const recentTxRows = db
      .prepare(
        `SELECT t.guid, t.post_date, t.description,
					CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) as amount,
					a.name as account_name,
					a.account_type
				FROM (
					SELECT guid, post_date, description
					FROM transactions
					WHERE post_date < date('now', '+1 day')
					ORDER BY post_date DESC
					LIMIT 10
				) t
				JOIN splits s ON s.tx_guid = t.guid
				JOIN accounts a ON a.guid = s.account_guid
				ORDER BY t.post_date DESC, t.guid`
      )
      .all() as {
      guid: string;
      post_date: string;
      description: string;
      amount: number;
      account_name: string;
      account_type: string;
    }[];

    // Group by transaction: prefer income/expense splits, fall back to any type
    const txSplits = new Map<
      string,
      {
        postDate: string;
        description: string;
        splits: { amount: number; accountName: string; accountType: string }[];
      }
    >();
    for (const row of recentTxRows) {
      let entry = txSplits.get(row.guid);
      if (!entry) {
        entry = { postDate: row.post_date, description: row.description, splits: [] };
        txSplits.set(row.guid, entry);
      }
      entry.splits.push({
        amount: Math.abs(row.amount),
        accountName: row.account_name,
        accountType: row.account_type
      });
    }
    const recentTransactions: DashboardTransaction[] = [];
    for (const [guid, entry] of txSplits) {
      let best = entry.splits.find(
        (s) => s.accountType === 'INCOME' || s.accountType === 'EXPENSE'
      );
      if (!best) {
        best = entry.splits.reduce((a, b) => (a.amount > b.amount ? a : b), entry.splits[0]);
      }
      if (best) {
        recentTransactions.push({
          guid,
          postDate: entry.postDate,
          description: entry.description,
          amount: best.amount,
          accountName: best.accountName,
          accountType: best.accountType
        });
      }
    }

    return {
      netWorth,
      netWorthTrend,
      monthlyBreakdown,
      topCategories,
      recentTransactions,
      currentMonthIncome: periodIncome,
      currentMonthExpenses: periodExpenses,
      currentMonthNet: periodIncome - periodExpenses
    };
  } finally {
    db.close();
  }
}
