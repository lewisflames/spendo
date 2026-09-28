import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { open, save } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { basename } from '@tauri-apps/api/path';
import { toast } from '$lib/components/ui/sonner';
import { activeBookPath, dbSession, setActiveBookPath, setDbSession } from '$lib/stores/db.svelte';
import { resetNav } from '$lib/stores/nav.svelte';
import { loadBookSettings, resetBookSettings } from '$lib/stores/book-settings.svelte';
import { appSettings } from '$lib/stores/settings.svelte';
import { confirmDiscardIfDirty, clearDiscardWaiver } from '$lib/stores/unsaved-changes.svelte';
import { closeBook, createBookAt, openBook, saveBookAs, type DataSummary } from '$lib/tauri';
import { ensureSpendoExtension } from '$lib/utils/book';
import { defaultCurrencyForLocale } from '$lib/utils/currency';
import { errMsg } from '$lib/utils/error';

const BOOK_FILTER = { name: 'Spendo book', extensions: ['spendo', 'db', 'sqlite'] };
const SAVE_FILTER = { name: 'Spendo book', extensions: ['spendo'] };

/**
 * Single frontend path for every book switch (sidebar menu, Settings, palette).
 * A module-level flag serializes the native dialogs and backend swaps; menu
 * items don’t render disabled state, so re-entry simply no-ops.
 */
let switching = false;

/** Keep book-scoped settings in sync after a swap; failures get a toast but
 * must not turn a successful switch into an error. */
async function reloadBookSettings() {
  try {
    await loadBookSettings(true);
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t load your book settings'));
  }
}

/** Shared success tail of every book switch: adopt the session, sync
 * book-scoped settings, announce. The `switching` guard is the caller’s job. */
async function activateBook(path: string, summary: DataSummary, message: string): Promise<void> {
  resetNav();
  setDbSession(summary);
  setActiveBookPath(path);
  await reloadBookSettings();
  toast.success(message);
}

/** Ask about in-flight form edits, then run the switch. A switch that fails
 * (or is cancelled) re-arms the guard so edits stay protected. */
async function withDiscardCheck(run: () => Promise<boolean>): Promise<boolean> {
  if (!(await confirmDiscardIfDirty())) return false;
  try {
    return await run();
  } finally {
    clearDiscardWaiver();
  }
}

/** Open a book at a known path (welcome drag-and-drop). Confirms before
 * abandoning in-flight form edits; failures toast and return false so callers
 * can keep the user on the welcome screen. */
export async function openBookAt(path: string): Promise<boolean> {
  return withDiscardCheck(() => openBookAtPath(path));
}

/**
 * Swap in a book whose file has already been chosen. Content changes, so
 * in-flight pages return home rather than holding on to stale accounts.
 */
async function openBookAtPath(path: string): Promise<boolean> {
  if (switching) return false;
  switching = true;
  try {
    const result = await openBook(path);
    await activateBook(path, result.summary, `Opened ${await basename(path)}`);
    await goto(resolve('/'), { replaceState: true });
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t open that book'));
    return false;
  } finally {
    switching = false;
  }
}

export async function openBookViaDialog(): Promise<boolean> {
  if (switching) return false;
  let selected: string | string[] | null;
  try {
    selected = await open({ multiple: false, filters: [BOOK_FILTER] });
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t open the file picker'));
    return false;
  }
  if (!selected || Array.isArray(selected)) return false;
  // Ask only after a file is chosen, so canceling the picker doesn’t waive edits.
  return withDiscardCheck(() => openBookAtPath(selected));
}

/** Create a book at a user-chosen path. `currency` defaults to the locale’s
 * when the caller (welcome screen) doesn’t offer a picker. */
export async function createBookViaDialog(currency?: string): Promise<boolean> {
  if (switching) return false;
  let target: string | null;
  try {
    target = await save({ defaultPath: 'book.spendo', filters: [SAVE_FILTER] });
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t open the save dialog'));
    return false;
  }
  if (!target) return false;
  // Ask only after a path is chosen, so canceling the picker doesn’t waive edits.
  const file = ensureSpendoExtension(target);
  const base = currency ?? defaultCurrencyForLocale(appSettings.value.locale);
  return withDiscardCheck(async () => {
    switching = true;
    try {
      const result = await createBookAt(file, base);
      await activateBook(file, result.summary, `Book created (${base})`);
      await goto(resolve('/'), { replaceState: true });
      return true;
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t create that book'));
      return false;
    } finally {
      switching = false;
    }
  });
}

export async function saveBookAsViaDialog(): Promise<boolean> {
  if (switching || !dbSession.value) return false;
  let target: string | null;
  try {
    target = await save({ defaultPath: 'book.spendo', filters: [SAVE_FILTER] });
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t open the save dialog'));
    return false;
  }
  if (!target) return false;
  const file = ensureSpendoExtension(target);
  switching = true;
  try {
    const result = await saveBookAs(file);
    await activateBook(file, result.summary, `Book saved as ${await basename(file)}`);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t save that book'));
    return false;
  } finally {
    switching = false;
  }
}

/** Writes are persisted live, but an open form may hold unsubmitted edits, so
 * confirm those first. The layout’s empty-session guard would redirect anyway;
 * `goto` makes it immediate. */
export async function closeActiveBook(): Promise<boolean> {
  if (switching) return false;
  if (!activeBookPath.value) {
    toast.error('No book open');
    return false;
  }
  return withDiscardCheck(async () => {
    switching = true;
    try {
      await closeBook();
      resetNav();
      setDbSession(null);
      setActiveBookPath(null);
      resetBookSettings();
      toast.success('Book closed');
      await goto(resolve('/'));
      return true;
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t close the book'));
      return false;
    } finally {
      switching = false;
    }
  });
}

/** The path-taking variant is the single implementation for callers that
 * already have a book path (settings book card). */
export async function copyBookPathAt(path: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(path);
    toast.success('Book path copied');
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t copy that path'));
    return false;
  }
}

export async function copyActiveBookPath(): Promise<boolean> {
  const path = activeBookPath.value;
  if (!path) return false;
  return copyBookPathAt(path);
}

/** Reveal the active book file in the OS file manager. */
export async function revealActiveBook(): Promise<boolean> {
  const path = activeBookPath.value;
  if (!path) return false;
  try {
    await revealItemInDir(path);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t open that folder'));
    return false;
  }
}
