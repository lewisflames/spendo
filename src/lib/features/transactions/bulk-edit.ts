import type { BulkAction, BulkEditResult, TagRef, TransactionRow } from '$lib/tauri';
import { plural } from '$lib/utils/format.svelte';

/** Tri-state of the header checkbox for the rows on this page. */
export function pageSelectionState(
  selected: Pick<Map<string, unknown>, 'has'>,
  rows: readonly { guid: string }[]
): 'all' | 'some' | 'none' {
  if (rows.length === 0) return 'none';
  let count = 0;
  for (const r of rows) if (selected.has(r.guid)) count += 1;
  if (count === 0) return 'none';
  return count === rows.length ? 'all' : 'some';
}

/** Rows from `fromGuid` to `toGuid` inclusive, in current row order. Falls
 * back to just the `toGuid` row when the anchor isn't on this page. */
export function rangeRows<T extends { guid: string }>(
  rows: readonly T[],
  fromGuid: string | null,
  toGuid: string
): T[] {
  const to = rows.findIndex((r) => r.guid === toGuid);
  if (to === -1) return [];
  const from = fromGuid === null ? -1 : rows.findIndex((r) => r.guid === fromGuid);
  if (from === -1) return [rows[to]];
  const [start, end] = from <= to ? [from, to] : [to, from];
  return rows.slice(start, end + 1);
}

/** Accounts posted to by the selected rows, most-used first (ties by name). */
export function selectionAccounts(
  rows: Iterable<Pick<TransactionRow, 'guid' | 'splits'>>
): { guid: string; count: number }[] {
  const counts = new Map<string, { count: number; name: string }>();
  for (const row of rows) {
    const seen = new Set<string>();
    for (const split of row.splits) {
      if (seen.has(split.accountGuid)) continue;
      seen.add(split.accountGuid);
      const entry = counts.get(split.accountGuid);
      if (entry) entry.count += 1;
      else counts.set(split.accountGuid, { count: 1, name: split.accountName });
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1].count - a[1].count || a[1].name.localeCompare(b[1].name))
    .map(([guid, { count }]) => ({ guid, count }));
}

/** Tags the selected rows carry, most-used first (ties by name). */
export function selectionTags(rows: Iterable<Pick<TransactionRow, 'tags'>>): TagRef[] {
  const counts = new Map<string, { tag: TagRef; count: number }>();
  for (const row of rows) {
    for (const tag of row.tags) {
      const entry = counts.get(tag.guid);
      if (entry) entry.count += 1;
      else counts.set(tag.guid, { tag, count: 1 });
    }
  }
  return [...counts.values()]
    .sort((a, b) => b.count - a.count || a.tag.name.localeCompare(b.tag.name))
    .map((e) => e.tag);
}

const DONE_VERB: Record<BulkAction['kind'], string> = {
  delete: 'Deleted',
  moveSplits: 'Moved',
  setDescription: 'Renamed',
  addTag: 'Tagged',
  removeTag: 'Untagged'
};

const NOTHING_TO_DO: Partial<Record<BulkAction['kind'], string>> = {
  moveSplits: 'Nothing to move. None of them use that account',
  addTag: 'Nothing changed. They all have that tag already',
  removeTag: 'Nothing to remove. None of them have that tag'
};

/** Toast text for a finished bulk edit. */
export function bulkResultMessage(kind: BulkAction['kind'], result: BulkEditResult): string {
  const locked =
    result.skippedLocked > 0
      ? `${plural(result.skippedLocked, 'cleared or reconciled transaction')} left as is`
      : '';
  if (result.changed === 0) {
    if (locked) return `Nothing changed. ${locked}`;
    return NOTHING_TO_DO[kind] ?? 'Nothing changed';
  }
  const done = `${DONE_VERB[kind]} ${plural(result.changed, 'transaction')}`;
  return locked ? `${done} · ${locked}` : done;
}
