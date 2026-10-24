import type { SpendingCategory, SpendingReport } from '$lib/tauri';

/** Categorical slots in `app.css` (`--series-1…8`), validated in this order. */
export const SERIES_SLOTS = 8;
export const OTHER_KEY = 'other';
export const OTHER_COLOR = 'var(--muted-foreground)';

export interface SpendingSeries {
  key: string;
  label: string;
  color: string;
  /** Category GUIDs drawn by this series (several for Other). */
  guids: string[];
}

export type SpendingChartRow = { month: string } & Record<string, number | string>;

/** Label for a category row; a parent's own splits read as its name plus "(direct)". */
export function categoryLabel(c: SpendingCategory): string {
  return c.ownSplits ? `${c.accountName} (direct)` : c.accountName;
}

/**
 * Chart series for the categories with activity in the window, largest
 * first. Each keeps its own hue while they fit the eight slots; past that
 * the top seven keep theirs and the rest fold into Other, so no hue is ever
 * generated or reused.
 */
export function buildSpendingSeries(categories: SpendingCategory[]): SpendingSeries[] {
  const active = categories.filter((c) => c.monthly.some((v) => v !== 0));
  const own = active.length <= SERIES_SLOTS ? active : active.slice(0, SERIES_SLOTS - 1);
  const series: SpendingSeries[] = own.map((c, i) => ({
    key: `c${i}`,
    label: categoryLabel(c),
    color: `var(--series-${i + 1})`,
    guids: [c.accountGuid]
  }));
  const rest = active.slice(own.length);
  if (rest.length > 0) {
    series.push({
      key: OTHER_KEY,
      label: 'Other',
      color: OTHER_COLOR,
      guids: rest.map((c) => c.accountGuid)
    });
  }
  return series;
}

/** One row per month with a value per series key (Other summed). */
export function buildSpendingRows(
  report: Pick<SpendingReport, 'months' | 'categories'>,
  series: SpendingSeries[]
): SpendingChartRow[] {
  const byGuid = new Map(report.categories.map((c) => [c.accountGuid, c]));
  return report.months.map((month, i) => {
    const row: SpendingChartRow = { month };
    for (const s of series) {
      row[s.key] = s.guids.reduce((sum, g) => sum + (byGuid.get(g)?.monthly[i] ?? 0), 0);
    }
    return row;
  });
}

/** Swatch color per category GUID, matching the chart (Other for the folded ones). */
export function seriesColorByGuid(series: SpendingSeries[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const s of series) for (const g of s.guids) map.set(g, s.color);
  return map;
}

/** Share of the period total, or null when there is nothing positive to share. */
export function sharePct(value: number, total: number): number | null {
  return total > 0 ? (value / total) * 100 : null;
}
