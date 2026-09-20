import { dbSession, setDbSession } from '$lib/stores/db.svelte';
import { fetchChangeHistory, redoHistoryEntry, undoHistoryEntry } from '$lib/tauri';
import type { ChangeEntry } from '$lib/tauri';
import { errMsg } from '$lib/utils/error';
import { toast } from '$lib/components/ui/sonner';

interface HistoryState {
  entries: ChangeEntry[];
  /** Newest change eligible for undo. */
  undoableId: number | null;
  /** Most recently undone change eligible for redo (LIFO). */
  redoableId: number | null;
  /** Older entries exist beyond the loaded pages. */
  hasMore: boolean;
  /** Total entries in the log, all pages. */
  total: number;
  loading: boolean;
  /** True while an older page is being appended (keeps the list visible). */
  loadingMore: boolean;
  error: string | null;
  /** True while an undo/redo is in flight (buttons disabled). */
  busy: boolean;
}

/**
 * Changes-history state shared by the side panel and the global Ctrl+Z /
 * Ctrl+Shift+Z shortcuts. Kept current while a book is open (the layout
 * refreshes it whenever the data summary's `historyRevision` changes), so
 * shortcuts work even with the panel closed.
 */
export const changeHistory = $state<{ value: HistoryState }>({
  value: {
    entries: [],
    undoableId: null,
    redoableId: null,
    hasMore: false,
    total: 0,
    loading: false,
    loadingMore: false,
    error: null,
    busy: false
  }
});

/** Replace fields wholesale (single notification), never mutate in place. */
function patch(partial: Partial<HistoryState>) {
  changeHistory.value = { ...changeHistory.value, ...partial };
}

let requestSeq = 0;

const EMPTY_PAGE = {
  entries: [] as ChangeEntry[],
  undoableId: null,
  redoableId: null,
  hasMore: false,
  total: 0,
  loadingMore: false
};

/**
 * Fetch the first page; stale responses from earlier requests are dropped.
 * State is invalidated up front, so undo/redo IDs from the previous book can
 * never be used while a refresh is in flight.
 */
export async function refreshChangeHistory(): Promise<void> {
  if (!dbSession.value) {
    requestSeq++;
    patch({ ...EMPTY_PAGE, loading: false, error: null });
    return;
  }
  const seq = ++requestSeq;
  patch({ ...EMPTY_PAGE, loading: true, error: null });
  try {
    const data = await fetchChangeHistory();
    if (seq !== requestSeq) return;
    patch({
      entries: data.entries,
      undoableId: data.undoableId,
      redoableId: data.redoableId,
      hasMore: data.hasMore,
      total: data.total,
      loading: false
    });
  } catch (e: unknown) {
    if (seq !== requestSeq) return;
    patch({
      undoableId: null,
      redoableId: null,
      loading: false,
      error: errMsg(e, 'Failed to load changes history')
    });
  }
}

/** Append the next page of older entries (panel "Load older"). */
export async function loadOlderHistory(): Promise<void> {
  const last = changeHistory.value.entries.at(-1);
  if (!last || !changeHistory.value.hasMore || changeHistory.value.loadingMore) return;
  const seq = requestSeq;
  patch({ loadingMore: true });
  try {
    const data = await fetchChangeHistory(last.id);
    if (seq !== requestSeq) return;
    // The page is keyed by `id < last.id`, so appends are already disjoint;
    // filter defensively without pulling in a reactive Set.
    const known = changeHistory.value.entries.map((e) => e.id);
    patch({
      entries: [
        ...changeHistory.value.entries,
        ...data.entries.filter((e) => !known.includes(e.id))
      ],
      hasMore: data.hasMore,
      total: data.total,
      loadingMore: false
    });
  } catch (e: unknown) {
    if (seq !== requestSeq) return;
    patch({ loadingMore: false });
    toast.error(errMsg(e, 'Failed to load older changes'));
  }
}

/** Label of a loaded entry, for labeled undo/redo toasts. */
function entryLabel(id: number): string | null {
  return changeHistory.value.entries.find((e) => e.id === id)?.label ?? null;
}

/** Undo the newest change, then refresh the log and the session summary. */
export async function undoLatestChange(): Promise<void> {
  const id = changeHistory.value.undoableId;
  if (id === null || changeHistory.value.busy) return;
  const label = entryLabel(id);
  patch({ busy: true });
  try {
    setDbSession(await undoHistoryEntry(id));
    toast.success(label ? `Undid: ${label}` : 'Change undone');
    await refreshChangeHistory();
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Undo failed'));
  } finally {
    patch({ busy: false });
  }
}

/** Redo the most recently undone change. */
export async function redoLatestChange(): Promise<void> {
  const id = changeHistory.value.redoableId;
  if (id === null || changeHistory.value.busy) return;
  const label = entryLabel(id);
  patch({ busy: true });
  try {
    setDbSession(await redoHistoryEntry(id));
    toast.success(label ? `Redid: ${label}` : 'Change redone');
    await refreshChangeHistory();
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Redo failed'));
  } finally {
    patch({ busy: false });
  }
}
