import type { GnuCashPreview } from '$lib/tauri';

/** Pending GnuCash import confirmation; opened by `import-export-actions`. */
export const gnucashImportDialog = $state({
  open: false,
  importing: false,
  preview: null as GnuCashPreview | null
});

export function openGnucashImportDialog(preview: GnuCashPreview): void {
  gnucashImportDialog.preview = preview;
  gnucashImportDialog.open = true;
}

export function closeGnucashImportDialog(): void {
  gnucashImportDialog.open = false;
  gnucashImportDialog.preview = null;
}
