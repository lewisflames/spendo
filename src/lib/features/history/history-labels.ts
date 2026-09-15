import type { ChangeEntry } from '$lib/tauri';
import { plural } from '$lib/utils/format.svelte';

export type LabelFields = Pick<
  ChangeEntry,
  'kind' | 'source' | 'transactionsAffected' | 'accountsAffected'
>;

/** Badge text for a change's origin: kind refined by source. */
export function sourceLabel(entry: LabelFields): string {
  if (entry.kind === 'revert') return 'Undo';
  if (entry.kind === 'redo') return 'Redo';
  if (entry.kind === 'import') return entry.source === 'csv' ? 'CSV' : 'Import';
  if (entry.source === 'schedule') return 'Scheduled';
  if (entry.kind === 'reconcile') return 'Reconcile';
  return 'Manual';
}

/**
 * Row-count summary for batch changes (imports, reconciliations); null for
 * single-entity edits where the label already says what changed.
 */
export function impactLabel(entry: LabelFields): string | null {
  if (entry.kind !== 'import' && entry.kind !== 'reconcile') return null;
  const parts: string[] = [];
  if (entry.transactionsAffected) {
    parts.push(plural(entry.transactionsAffected, 'transaction'));
  }
  if (entry.accountsAffected) {
    parts.push(plural(entry.accountsAffected, 'account'));
  }
  return parts.length > 0 ? parts.join(' · ') : null;
}
