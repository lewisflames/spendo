/**
 * Shared create-account dialog. Mounted once in the layout and opened from the
 * command palette and empty states so account creation no longer needs a
 * dedicated accounts route.
 */
export const accountDialog = $state({ value: false });

export function openAccountDialog(): void {
  accountDialog.value = true;
}
