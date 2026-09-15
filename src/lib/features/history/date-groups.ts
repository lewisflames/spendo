import { parseLocalDate } from '$lib/utils/format.svelte';

export type HistoryGroupKey =
  | 'today'
  | 'yesterday'
  | 'thisWeek'
  | 'lastWeek'
  | 'thisMonth'
  | 'lastMonth'
  | 'thisYear'
  | 'older';

export interface HistoryGroup<T> {
  key: HistoryGroupKey;
  label: string;
  entries: T[];
}

const GROUP_LABELS: Record<HistoryGroupKey, string> = {
  today: 'Today',
  yesterday: 'Yesterday',
  thisWeek: 'This week',
  lastWeek: 'Last week',
  thisMonth: 'This month',
  lastMonth: 'Last month',
  thisYear: 'Earlier this year',
  older: 'Older'
};

/** Local midnight for a calendar date (month/date may under-flow; Date normalizes). */
function localMidnight(year: number, month: number, date: number): number {
  return new Date(year, month, date).getTime();
}

/** Local midnight of the Monday starting the week that contains `d`. */
function startOfWeek(d: Date): number {
  const mondayOffset = (d.getDay() + 6) % 7;
  return localMidnight(d.getFullYear(), d.getMonth(), d.getDate() - mondayOffset);
}

/**
 * Relative bucket for a change timestamp. Buckets are checked newest-first, so
 * e.g. yesterday stays "Yesterday" even when it falls in last week, and early
 * this week's days read "Last week" rather than "This month".
 */
export function relativeDateGroup(occurredAt: string, now: Date = new Date()): HistoryGroupKey {
  const t = parseLocalDate(occurredAt).getTime();
  const year = now.getFullYear();
  const month = now.getMonth();
  const date = now.getDate();
  if (t >= localMidnight(year, month, date)) return 'today';
  if (t >= localMidnight(year, month, date - 1)) return 'yesterday';
  if (t >= startOfWeek(now)) return 'thisWeek';
  // Re-anchoring one week back to its Monday gives last week's start without
  // fixed 7-day arithmetic (which DST could shift by an hour).
  if (t >= startOfWeek(new Date(year, month, date - 7))) return 'lastWeek';
  if (t >= localMidnight(year, month, 1)) return 'thisMonth';
  if (t >= localMidnight(year, month - 1, 1)) return 'lastMonth';
  if (t >= localMidnight(year, 0, 1)) return 'thisYear';
  return 'older';
}

/**
 * Bucket entries by relative date, preserving input order (newest first).
 * Entries sharing a bucket merge into the first-seen group.
 */
export function groupHistoryEntries<T extends { occurredAt: string }>(
  entries: readonly T[],
  now: Date = new Date()
): HistoryGroup<T>[] {
  const buckets = new Map<HistoryGroupKey, T[]>();
  for (const entry of entries) {
    const key = relativeDateGroup(entry.occurredAt, now);
    const bucket = buckets.get(key);
    if (bucket) bucket.push(entry);
    else buckets.set(key, [entry]);
  }
  return [...buckets].map(([key, groupEntries]) => ({
    key,
    label: GROUP_LABELS[key],
    entries: groupEntries
  }));
}
