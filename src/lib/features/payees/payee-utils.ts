import { resolve } from '$app/paths';
import { plural } from '$lib/utils/format.svelte';
import type { PayeeSummary } from '$lib/tauri';

/** Mirrored from `payees.rs` (`MAX_PAYEE_LEN`). */
export const MAX_PAYEE_LEN = 100;

/** Trim and collapse inner whitespace, like the backend does before saving. */
export function normalizePayeeName(raw: string): string {
  return raw.trim().split(/\s+/).filter(Boolean).join(' ');
}

/** Why `name` (already normalized) can't be a payee, or `null` when it can. Blank means none. */
export function payeeNameError(name: string): string | null {
  if ([...name].length > MAX_PAYEE_LEN) return `Keep payee names under ${MAX_PAYEE_LEN} characters`;
  return null;
}

/** Suggestions for a payee combobox, most used first. */
export function payeeOptions(
  payees: readonly PayeeSummary[]
): { description: string; hint: string }[] {
  return payees.map((p) => ({
    description: p.name,
    hint: plural(p.transactionCount, 'transaction')
  }));
}

/** The transactions list filtered to one payee. Query strings can't go through `resolve()`. */
export function payeeListHref(guid: string): string {
  return `${resolve('/transactions')}?payee=${encodeURIComponent(guid)}`;
}
