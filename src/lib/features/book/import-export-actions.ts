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
  exportRealizedGainsCsv,
  exportTaxSummaryCsv,
  exportTransactionsCsv,
  importGnucash,
  previewGnucashImport
} from '$lib/tauri';
import { ensureCsvExtension } from '$lib/utils/book';
import { todayStr } from '$lib/utils/date-utils.svelte';
import { errMsg } from '$lib/utils/error';
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
 * dialogs; menu items don’t render disabled state, so re-entry simply no-ops.
 */
let busy = false;

/** The CSV wizard needs a page (mapping + preview); GnuCash import doesn’t. */
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
    toast.error(errMsg(e, 'We couldn’t open the file picker'));
    return false;
  }
  if (!selected || Array.isArray(selected)) return false;
  try {
    return await openPreview(selected, await basename(selected));
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t preview that file'));
    return false;
  }
}

/** Runs the merge for the pending preview once the user confirms. */
export async function confirmGnucashImport(deleteOrphans: boolean): Promise<boolean> {
  const preview = gnucashImportDialog.value.preview;
  if (!preview || busy || !dbSession.value) return false;
  busy = true;
  gnucashImportDialog.value.importing = true;
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
    if (transactionsChanged) {
      const parts = [`${result.transactionsAdded} added`];
      if (result.transactionsUpdated) parts.push(`${result.transactionsUpdated} updated`);
      if (result.transactionsOrphaned) parts.push(`${result.transactionsOrphaned} removed`);
      toast.success(`Imported ${preview.filename} (${parts.join(' · ')})`);
    } else {
      toast.warning('No new data imported');
    }
    toastPriceResult(result.prices, { fixHint: true });
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, 'We couldn’t finish that import'));
    return false;
  } finally {
    toast.dismiss(loading);
    gnucashImportDialog.value.importing = false;
    busy = false;
  }
}

/** Export preferences live in the book; load them on demand so the menu and
 * palette paths don’t depend on the Settings page having been visited. A failed
 * load falls back to defaults rather than blocking the export. */
async function exportOptions() {
  try {
    await loadBookSettings();
  } catch {
    // Defaults below are good enough to save the user’s data.
  }
  return {
    delimiter: bookSettings.value.exportCsvDelimiter,
    dateFormat: bookSettings.value.exportDateFormat,
    locale: appSettings.value.locale
  };
}

/** Shared save-dialog tail: confirm the path, run the export and announce it.
 * `run` may return the toast message itself when it has more to report than
 * the filename. */
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
      toast.error(errMsg(e, 'We couldn’t open the save dialog'));
      return false;
    }
    // Null means the user cancelled — stay silent.
    if (!target) return false;
    const file = ensureExtension(target);
    const filename = await basename(file);
    const loading = toast.loading(`Exporting ${filename}…`);
    try {
      const message = await run(file);
      toast.success(message ?? `Saved ${filename}`);
    } finally {
      toast.dismiss(loading);
    }
    return true;
  } catch (e: unknown) {
    toast.error(errMsg(e, `We couldn’t export ${label}`));
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

/** One year's realized gains, one row per lot a sale consumed. */
export async function exportRealizedGainsCsvViaDialog(year: number): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const opts = await exportOptions();
  const pattern = opts.dateFormat === 'locale' ? exportDatePattern(opts.locale ?? 'en-IE') : null;
  return saveExport(
    exportFilename(`realized-gains-${year}`, bookStem(), todayStr()),
    CSV_FILTER,
    ensureCsvExtension,
    (path) =>
      exportRealizedGainsCsv({
        path,
        year,
        delimiter: opts.delimiter,
        dateFormat: opts.dateFormat,
        dateParts: pattern?.parts ?? null,
        dateSeparator: pattern?.separator ?? null
      }),
    'realized gains'
  );
}

/** One year's tax summary: account lines per section, then the totals. */
export async function exportTaxSummaryCsvViaDialog(year: number): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  const opts = await exportOptions();
  return saveExport(
    exportFilename(`tax-summary-${year}`, bookStem(), todayStr()),
    CSV_FILTER,
    ensureCsvExtension,
    (path) => exportTaxSummaryCsv({ path, year, delimiter: opts.delimiter }),
    'tax summary'
  );
}
