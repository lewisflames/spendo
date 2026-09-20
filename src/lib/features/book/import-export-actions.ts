import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { basename } from '@tauri-apps/api/path';
import { open, save } from '@tauri-apps/plugin-dialog';
import { toast } from '$lib/components/ui/sonner';
import { toastPriceResult } from '$lib/features/portfolio/price-utils';
import { bookSettings, loadBookSettings } from '$lib/stores/book-settings.svelte';
import { dbSession, setDbSession } from '$lib/stores/db.svelte';
import { appSettings } from '$lib/stores/settings.svelte';
import { importGnucash, reimportLast, writeTextFile } from '$lib/tauri';
import { ensureCsvExtension } from '$lib/utils/book';
import { errMsg } from '$lib/utils/error';
import {
  ACCOUNTS_EXPORT_FILENAME,
  TRANSACTIONS_EXPORT_FILENAME,
  buildAccountsExport,
  buildTransactionsExport
} from './exporters';

const GNUCASH_FILTER = { name: 'GnuCash', extensions: ['gnucash'] };
const CSV_FILTER = { name: 'CSV', extensions: ['csv'] };

/**
 * Single frontend path for every book-scoped data transfer (sidebar book menu,
 * command palette). A module-level flag serializes the native dialogs; menu
 * items don't render disabled state, so re-entry simply no-ops.
 */
let busy = false;

/** The CSV wizard needs a page (mapping + preview); GnuCash import doesn't. */
export function openCsvImport(): void {
  void goto(resolve('/import/csv'));
}

/** Pick a `.gnucash` file and merge it into the active book. Returns false on
 * cancel, guard or failure so callers can stay put. */
export async function importGnucashViaDialog(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  let selected: string | string[] | null;
  try {
    selected = await open({ multiple: false, filters: [GNUCASH_FILTER] });
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Failed to open file dialog'));
    return false;
  }
  if (!selected || Array.isArray(selected)) return false;
  busy = true;
  try {
    const filename = await basename(selected);
    const result = await importGnucash(selected, filename);
    setDbSession(result.summary);
    toast.success(`Imported ${filename}`);
    toastPriceResult(result.prices, { fixHint: true });
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Import failed'));
    return false;
  } finally {
    busy = false;
  }
}

/** Re-run the most recent GnuCash import if its source file still exists. */
export async function reimportLastGnucash(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  busy = true;
  try {
    const result = await reimportLast();
    setDbSession(result.summary);
    if (
      result.accountsAdded ||
      result.transactionsAdded ||
      result.splitsAdded ||
      result.commoditiesAdded ||
      result.accountsUpdated ||
      result.transactionsUpdated ||
      result.splitsUpdated
    ) {
      toast.success('Reimport complete');
    } else {
      toast.warning('No new data imported');
    }
    toastPriceResult(result.prices, { fixHint: true });
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Reimport failed'));
    return false;
  } finally {
    busy = false;
  }
}

/** Export preferences live in the book; load them on demand so the menu and
 * palette paths don't depend on the Settings page having been visited. A failed
 * load falls back to defaults rather than blocking the export. */
async function exportOptions() {
  try {
    await loadBookSettings();
  } catch {
    // Defaults below are good enough to save the user's data.
  }
  return {
    delimiter: bookSettings.value.exportCsvDelimiter,
    dateFormat: bookSettings.value.exportDateFormat,
    locale: appSettings.value.locale
  };
}

/** Shared save-dialog tail: build the CSV, write it and announce the file. */
async function saveExport(
  defaultPath: string,
  build: () => Promise<string>,
  label: string
): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  busy = true;
  try {
    let target: string | null;
    try {
      target = await save({ defaultPath, filters: [CSV_FILTER] });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return false;
    }
    // Null means the user cancelled — stay silent.
    if (!target) return false;
    const file = ensureCsvExtension(target);
    await writeTextFile(file, await build());
    toast.success(`Saved ${await basename(file)}`);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, `Failed to export ${label}`));
    return false;
  } finally {
    busy = false;
  }
}

/** Full-book transactions CSV dump (one row per split, GnuCash-compatible). */
export async function exportTransactionsCsvViaDialog(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const opts = await exportOptions();
  return saveExport(
    TRANSACTIONS_EXPORT_FILENAME,
    () => buildTransactionsExport(opts),
    'transactions'
  );
}

/** Full account-tree CSV dump in parent-before-child order. */
export async function exportAccountsCsvViaDialog(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const opts = await exportOptions();
  return saveExport(ACCOUNTS_EXPORT_FILENAME, () => buildAccountsExport(opts), 'accounts');
}
