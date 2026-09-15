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
  loading: boolean;
  error: string | null;
  /** True while an undo/redo is in flight (buttons disabled). */
  busy: boolean;
}

/**
 * Changes-history state shared by the side panel and the global Ctrl+Z /
 * Ctrl+Shift+Z shortcuts. Kept current while a book is open (the layout
 * refreshes it whenever the data summary changes), so shortcuts work even
 * with the panel closed.
 */
export const changeHistory = $state<{ value: HistoryState }>({
  value: {
    entries: [],
    undoableId: null,
    redoableId: null,
    loading: false,
    error: null,
    busy: false
  }
});

/** Replace fields wholesale (single notification), never mutate in place. */
function patch(partial: Partial<HistoryState>) {
  changeHistory.value = { ...changeHistory.value, ...partial };
}

let requestSeq = 0;

/** Fetch the full log; stale responses from earlier requests are dropped. */
export async function refreshChangeHistory(): Promise<void> {
  if (!dbSession.value) {
    requestSeq++;
    patch({ entries: [], undoableId: null, redoableId: null, loading: false, error: null });
    return;
  }
  const seq = ++requestSeq;
  patch({ loading: true, error: null });
  try {
    const data = await fetchChangeHistory();
    if (seq !== requestSeq) return;
    patch({
      entries: data.entries,
      undoableId: data.undoableId,
      redoableId: data.redoableId,
      loading: false
    });
  } catch (e: unknown) {
    if (seq !== requestSeq) return;
    patch({ loading: false, error: errMsg(e, 'Failed to load changes history') });
  }
}

/** Undo the newest change, then refresh the log and the session summary. */
export async function undoLatestChange(): Promise<void> {
  const id = changeHistory.value.undoableId;
  if (id === null || changeHistory.value.busy) return;
  patch({ busy: true });
  try {
    setDbSession(await undoHistoryEntry(id));
    toast.success('Change undone');
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
  patch({ busy: true });
  try {
    setDbSession(await redoHistoryEntry(id));
    toast.success('Change redone');
    await refreshChangeHistory();
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Redo failed'));
  } finally {
    patch({ busy: false });
  }
}
