import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { basename } from '@tauri-apps/api/path';
import { open, save } from '@tauri-apps/plugin-dialog';
import { toast } from '$lib/components/ui/sonner';
import { toastPriceResult } from '$lib/features/portfolio/price-utils';
import { bookSettings, loadBookSettings } from '$lib/stores/book-settings.svelte';
import { activeBookPath, dbSession, setDbSession } from '$lib/stores/db.svelte';
import { appSettings } from '$lib/stores/settings.svelte';
import {
  exportAccountsCsv,
  exportGnuCash,
  exportTransactionsCsv,
  importGnucash,
  previewGnucashImport
} from '$lib/tauri';
import { ensureCsvExtension, ensureGnuCashExtension } from '$lib/utils/book';
import { todayStr } from '$lib/utils/date-utils.svelte';
import { errMsg } from '$lib/utils/error';
import { plural } from '$lib/utils/format.svelte';
import { exportDatePattern, exportFilename } from './exporters';
import {
  closeGnucashImportDialog,
  gnucashImportDialog,
  openGnucashImportDialog
} from './gnucash-import-dialog-state.svelte';

const GNUCASH_FILTER = { name: 'GnuCash', extensions: ['gnucash'] };
const CSV_FILTER = { name: 'CSV', extensions: ['csv'] };

/**
 * Single frontend path for every book-scoped data transfer (sidebar book menu,
 * command palette). A module-level flag serializes the native
 * dialogs; menu items don't render disabled state, so re-entry simply no-ops.
 */
let busy = false;

/** The CSV wizard needs a page (mapping + preview); GnuCash import doesn't. */
export function openCsvImport(): void {
  void goto(resolve('/import/csv'));
}

/** Hash, dry-run and open the confirmation dialog for a GnuCash source. */
async function openPreview(path: string, filename: string): Promise<boolean> {
  const loading = toast.loading(`Reading ${filename}…`);
  try {
    const preview = await previewGnucashImport(path, filename);
    openGnucashImportDialog(preview);
    return true;
  } finally {
    toast.dismiss(loading);
  }
}

/** Pick a `.gnucash` file, preview it and open the confirmation dialog. */
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
  try {
    return await openPreview(selected, await basename(selected));
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Import preview failed'));
    return false;
  }
}

/** Runs the merge for the pending preview once the user confirms. */
export async function confirmGnucashImport(deleteOrphans: boolean): Promise<boolean> {
  const preview = gnucashImportDialog.preview;
  if (!preview || busy || !dbSession.value) return false;
  busy = true;
  gnucashImportDialog.importing = true;
  const loading = toast.loading(`Importing ${preview.filename}…`);
  try {
    const result = await importGnucash({
      path: preview.path,
      filename: preview.filename,
      sha256: preview.sha256,
      deleteOrphans
    });
    setDbSession(result.summary);
    closeGnucashImportDialog();
    const transactionsChanged =
      result.transactionsAdded + result.transactionsUpdated + result.transactionsOrphaned;
    const schedulesChanged = result.schedulesAdded + result.schedulesUpdated;
    if (transactionsChanged || schedulesChanged) {
      const parts: string[] = [];
      if (transactionsChanged) {
        parts.push(`${result.transactionsAdded} added`);
        if (result.transactionsUpdated) parts.push(`${result.transactionsUpdated} updated`);
        if (result.transactionsOrphaned) parts.push(`${result.transactionsOrphaned} removed`);
      }
      if (result.schedulesAdded) parts.push(`${plural(result.schedulesAdded, 'schedule')} added`);
      if (result.schedulesUpdated) {
        parts.push(`${plural(result.schedulesUpdated, 'schedule')} updated`);
      }
      if (result.schedulesSkipped) {
        parts.push(`${plural(result.schedulesSkipped, 'schedule')} skipped`);
      }
      toast.success(`Imported ${preview.filename} — ${parts.join(' · ')}`);
    } else {
      toast.warning('No new data imported');
    }
    toastPriceResult(result.prices, { fixHint: true });
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Import failed'));
    return false;
  } finally {
    toast.dismiss(loading);
    gnucashImportDialog.importing = false;
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

/** Shared save-dialog tail: confirm the path, run the export and announce it.
 * `run` may return the toast message itself when it has more to report than
 * the filename (the GnuCash export summarizes what landed in the file). */
async function saveExport(
  defaultPath: string,
  filter: { name: string; extensions: string[] },
  ensureExtension: (path: string) => string,
  run: (path: string) => Promise<string | void>,
  label: string
): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  busy = true;
  try {
    let target: string | null;
    try {
      target = await save({ defaultPath, filters: [filter] });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return false;
    }
    // Null means the user cancelled — stay silent.
    if (!target) return false;
    const file = ensureExtension(target);
    const message = await run(file);
    toast.success(message ?? `Saved ${await basename(file)}`);
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, `Failed to export ${label}`));
    return false;
  } finally {
    busy = false;
  }
}

/** Basename of the active book for the default export filename. */
function bookStem(): string {
  const path = activeBookPath.value ?? '';
  return path.split(/[\\/]/).pop() || 'spendo';
}

/** Full-book transactions CSV dump (one row per split, GnuCash-compatible). */
export async function exportTransactionsCsvViaDialog(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const opts = await exportOptions();
  const pattern = opts.dateFormat === 'locale' ? exportDatePattern(opts.locale ?? 'en-IE') : null;
  return saveExport(
    exportFilename('transactions', bookStem(), todayStr()),
    CSV_FILTER,
    ensureCsvExtension,
    (path) =>
      exportTransactionsCsv({
        path,
        delimiter: opts.delimiter,
        dateFormat: opts.dateFormat,
        dateParts: pattern?.parts ?? null,
        dateSeparator: pattern?.separator ?? null
      }),
    'transactions'
  );
}

/** Full account-tree CSV dump in parent-before-child order. */
export async function exportAccountsCsvViaDialog(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const opts = await exportOptions();
  return saveExport(
    exportFilename('accounts', bookStem(), todayStr()),
    CSV_FILTER,
    ensureCsvExtension,
    (path) => exportAccountsCsv({ path, delimiter: opts.delimiter }),
    'accounts'
  );
}

/** Full-book GnuCash SQLite export (accounts, transactions, schedules, prices,
 * budgets). Receipts have no GnuCash equivalent and stay in this book. */
export async function exportGnuCashViaDialog(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const stem = bookStem().replace(/\.spendo$/i, '') || 'spendo';
  return saveExport(
    `${stem}.gnucash`,
    GNUCASH_FILTER,
    ensureGnuCashExtension,
    async (path) => {
      const result = await exportGnuCash(path);
      if (result.attachmentsSkipped > 0) {
        toast.warning(
          `${plural(result.attachmentsSkipped, 'receipt')} not exported — GnuCash cannot store attached files`
        );
      }
      if (result.roundedAmounts > 0) {
        toast.warning(
          `${plural(result.roundedAmounts, 'amount')} have more than 2 decimals — GnuCash rounds them on open`
        );
      }
      const parts = [
        plural(result.transactions, 'transaction'),
        plural(result.accounts, 'account')
      ];
      if (result.schedules > 0) parts.push(plural(result.schedules, 'schedule'));
      if (result.prices > 0) parts.push(plural(result.prices, 'price'));
      if (result.futureDated > 0) parts.push(`${result.futureDated} future-dated`);
      return `Saved ${await basename(path)} — ${parts.join(' · ')}`;
    },
    'GnuCash file'
  );
}
