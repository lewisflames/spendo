import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { open, save } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { basename } from '@tauri-apps/api/path';
import { toast } from '$lib/components/ui/sonner';
import { activeBookPath, dbSession, setActiveBookPath, setDbSession } from '$lib/stores/db.svelte';
import { loadBookSettings, resetBookSettings } from '$lib/stores/book-settings.svelte';
import { appSettings } from '$lib/stores/settings.svelte';
import { closeBook, createBookAt, openBook, saveBookAs } from '$lib/tauri';
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
  switching = true;
  try {
    const result = await openBook(selected);
    setDbSession(result.summary);
    setActiveBookPath(selected);
    await reloadBookSettings();
    toast.success(`Opened ${await basename(selected)}`);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to open book'));
    return false;
  } finally {
    switching = false;
  }
}

export async function createBookViaDialog(): Promise<boolean> {
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
  const currency = defaultCurrencyForLocale(appSettings.value.locale);
  switching = true;
  try {
    const result = await createBookAt(file, currency);
    setDbSession(result.summary);
    setActiveBookPath(file);
    await reloadBookSettings();
    toast.success(`Book created (${currency})`);
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
    setDbSession(result.summary);
    setActiveBookPath(file);
    await reloadBookSettings();
    toast.success(`Book saved as ${await basename(file)}`);
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

export async function revealActiveBook(): Promise<void> {
  const path = activeBookPath.value;
  if (!path) return;
  try {
    await revealItemInDir(path);
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to open file manager'));
  }
}

export async function copyActiveBookPath(): Promise<boolean> {
  const path = activeBookPath.value;
  if (!path) return false;
  try {
    await navigator.clipboard.writeText(path);
    toast.success('Book path copied');
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to copy path'));
    return false;
  }
}
