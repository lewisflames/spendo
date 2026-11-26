import { resolve } from '$app/paths';
import type { ConflictField, SyncConflictCopy } from '$lib/tauri';

/** Most dismissed copies remembered; older ones drop off first. */
export const MAX_DISMISSED = 50;

/** Stored dismissals: a list of conflict file paths. Anything else is dropped. */
export function parseDismissed(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  return raw.filter((p): p is string => typeof p === 'string' && p !== '').slice(-MAX_DISMISSED);
}

/** Add `path` (once) at the end, keeping the newest `MAX_DISMISSED`. */
export function withDismissed(list: string[], path: string): string[] {
  return [...list.filter((p) => p !== path), path].slice(-MAX_DISMISSED);
}

/** Copies the user hasn't dismissed, in the backend's newest-first order. */
export function visibleConflicts(
  copies: SyncConflictCopy[],
  dismissed: readonly string[]
): SyncConflictCopy[] {
  const hidden = new Set(dismissed);
  return copies.filter((c) => !hidden.has(c.path));
}

const FIELD_LABELS: Record<ConflictField, string> = {
  date: 'Date',
  description: 'Description',
  payee: 'Payee',
  number: 'Number',
  notes: 'Notes',
  tags: 'Tags',
  splits: 'Amounts or accounts',
  status: 'Reconcile status'
};

export function conflictFieldLabel(field: ConflictField): string {
  return FIELD_LABELS[field];
}

/** The review page for one copy. Query strings can't go through `resolve()`. */
export function conflictReviewHref(fileName: string): string {
  return `${resolve('/sync-conflict')}?file=${encodeURIComponent(fileName)}`;
}
