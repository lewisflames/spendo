import type { GnuCashPreview } from '$lib/tauri';

/** Pending GnuCash import confirmation; opened by `import-export-actions`. */
export const gnucashImportDialog = $state<{
  value: {
    open: boolean;
    importing: boolean;
    preview: GnuCashPreview | null;
  };
}>({ value: { open: false, importing: false, preview: null } });

export function openGnucashImportDialog(preview: GnuCashPreview): void {
  gnucashImportDialog.value.preview = preview;
  gnucashImportDialog.value.open = true;
}

export function closeGnucashImportDialog(): void {
  gnucashImportDialog.value.open = false;
  gnucashImportDialog.value.preview = null;
}
