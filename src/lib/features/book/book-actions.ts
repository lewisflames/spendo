import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { open, save } from '@tauri-apps/plugin-dialog';
import { basename } from '@tauri-apps/api/path';
import { toast } from '$lib/components/ui/sonner';
import { activeBookPath, dbSession, setActiveBookPath, setDbSession } from '$lib/stores/db.svelte';
import { loadBookSettings, resetBookSettings } from '$lib/stores/book-settings.svelte';
import { appSettings } from '$lib/stores/settings.svelte';
import { closeBook, createBookAt, openBook, saveBookAs, type DataSummary } from '$lib/tauri';
import { ensureSpendoExtension } from '$lib/utils/book';
import { defaultCurrencyForLocale } from '$lib/utils/currency';
import { errMsg } from '$lib/utils/error';

const BOOK_FILTER = { name: 'Spendo book', extensions: ['spendo', 'db', 'sqlite'] };
const SAVE_FILTER = { name: 'Spendo book', extensions: ['spendo'] };

/**
 * Single frontend path for every book switch (sidebar menu, Settings, palette).
 * A module-level flag serializes the native dialogs and backend swaps; menu
 * items don't render disabled state, so re-entry simply no-ops.
 */
let switching = false;

/** Keep book-scoped settings in sync after a swap; failures get a toast but
 * must not turn a successful switch into an error. */
async function reloadBookSettings() {
  try {
    await loadBookSettings(true);
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to load book settings'));
  }
}

/** Shared success tail of every book switch: adopt the session, sync
 * book-scoped settings, announce. The `switching` guard is the caller's job. */
async function activateBook(path: string, summary: DataSummary, message: string): Promise<void> {
  setDbSession(summary);
  setActiveBookPath(path);
  await reloadBookSettings();
  toast.success(message);
}

/** Open a book at a known path (welcome drag-and-drop). Failures toast and
 * return false so callers can keep the user on the welcome screen. */
export async function openBookAt(path: string): Promise<boolean> {
  if (switching) return false;
  switching = true;
  try {
    const result = await openBook(path);
    await activateBook(path, result.summary, `Opened ${await basename(path)}`);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to open book'));
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
    toast.error(errMsg(e, 'Failed to open file dialog'));
    return false;
  }
  if (!selected || Array.isArray(selected)) return false;
  return openBookAt(selected);
}

/** Create a book at a user-chosen path. `currency` defaults to the locale's
 * when the caller (welcome screen) doesn't offer a picker. */
export async function createBookViaDialog(currency?: string): Promise<boolean> {
  if (switching) return false;
  let target: string | null;
  try {
    target = await save({ defaultPath: 'book.spendo', filters: [SAVE_FILTER] });
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to open save dialog'));
    return false;
  }
  if (!target) return false;
  const file = ensureSpendoExtension(target);
  const base = currency ?? defaultCurrencyForLocale(appSettings.value.locale);
  switching = true;
  try {
    const result = await createBookAt(file, base);
    await activateBook(file, result.summary, `Book created (${base})`);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to create book'));
    return false;
  } finally {
    switching = false;
  }
}

export async function saveBookAsViaDialog(): Promise<boolean> {
  if (switching || !dbSession.value) return false;
  let target: string | null;
  try {
    target = await save({ defaultPath: 'book.spendo', filters: [SAVE_FILTER] });
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to open save dialog'));
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
    toast.error(errMsg(e, 'Failed to save book'));
    return false;
  } finally {
    switching = false;
  }
}

/** Writes are persisted live, so closing needs no confirmation; the layout's
 * empty-session guard would redirect anyway — `goto` makes it immediate. */
export async function closeActiveBook(): Promise<boolean> {
  if (switching) return false;
  if (!activeBookPath.value) {
    toast.error('No book open');
    return false;
  }
  switching = true;
  try {
    await closeBook();
    setDbSession(null);
    setActiveBookPath(null);
    resetBookSettings();
    toast.success('Book closed');
    await goto(resolve('/'));
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to close book'));
    return false;
  } finally {
    switching = false;
  }
}

/** The path-taking variant is the single implementation for callers that
 * already have a book path (settings book card). */
export async function copyBookPathAt(path: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(path);
    toast.success('Book path copied');
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to copy path'));
    return false;
  }
}

export async function copyActiveBookPath(): Promise<boolean> {
  const path = activeBookPath.value;
  if (!path) return false;
  return copyBookPathAt(path);
}
