import { listSyncConflicts, type SyncConflictCopy } from '$lib/tauri';
import { parseDismissed, withDismissed } from '$lib/features/sync-conflicts/sync-conflict-utils';

const STORAGE_KEY = 'spendo-dismissed-sync-conflicts';

function load(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === null ? [] : parseDismissed(JSON.parse(raw));
  } catch {
    return [];
  }
}

/** Sync conflict copies next to the open book, newest first. */
export const syncConflicts = $state<{ value: SyncConflictCopy[] }>({ value: [] });

/** Conflict file paths the user dismissed; the files themselves stay. */
export const dismissedSyncConflicts = $state<{ value: string[] }>({ value: load() });

let seq = 0;

/**
 * Look for conflict copies next to `path` (the open book), or clear the list
 * without one. The banner is a hint, so failures just leave it empty; only
 * the latest call's answer is kept.
 */
export async function refreshSyncConflicts(path: string | null): Promise<void> {
  const mine = ++seq;
  if (!path) {
    syncConflicts.value = [];
    return;
  }
  try {
    const copies = await listSyncConflicts();
    if (mine === seq) syncConflicts.value = copies;
  } catch {
    if (mine === seq) syncConflicts.value = [];
  }
}

export function dismissSyncConflict(path: string) {
  dismissedSyncConflicts.value = withDismissed(dismissedSyncConflicts.value, path);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(dismissedSyncConflicts.value));
  } catch {
    // Storage full/blocked — the dismissal lasts for this session only.
  }
}
