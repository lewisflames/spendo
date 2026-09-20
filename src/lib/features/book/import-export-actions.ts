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
  exportTransactionsCsv,
  fetchLastImportSource,
  importGnucash,
  previewGnucashImport,
  type LastImportSource
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
 * command palette, history panel). A module-level flag serializes the native
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

/** Re-run a remembered path; when it moved, let the user locate the file. */
async function previewOrLocate(path: string, filename: string): Promise<boolean> {
  try {
    return await openPreview(path, filename);
  } catch (e: unknown) {
    toast.warning(errMsg(e, 'Source file not found — pick it again'));
    return importGnucashViaDialog();
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

/** Preview the newest GnuCash source (menu's "Re-import last file"). */
export async function reimportLastGnucash(): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  let last: LastImportSource | null;
  try {
    last = await fetchLastImportSource();
  } catch (e: unknown) {
    toast.error(errMsg(e, 'Could not read the last import'));
    return false;
  }
  if (!last) {
    toast.warning('No previous GnuCash import in this book');
    return false;
  }
  return previewOrLocate(last.path, last.filename);
}

/** Preview a specific source (history "Re-run import"). */
export async function reimportGnucashPath(path: string, filename: string): Promise<boolean> {
  if (busy || !dbSession.value) return false;
  return previewOrLocate(path, filename);
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
    const changed =
      result.transactionsAdded + result.transactionsUpdated + result.transactionsOrphaned;
    if (changed) {
      const parts = [`${result.transactionsAdded} added`];
      if (result.transactionsUpdated) parts.push(`${result.transactionsUpdated} updated`);
      if (result.transactionsOrphaned) parts.push(`${result.transactionsOrphaned} removed`);
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

/** Shared save-dialog tail: confirm the path, run the export and announce it. */
async function saveExport(
  defaultPath: string,
  run: (path: string) => Promise<void>,
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
    await run(file);
    toast.success(`Saved ${await basename(file)}`);
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
    (path) => exportAccountsCsv({ path, delimiter: opts.delimiter }),
    'accounts'
  );
}
