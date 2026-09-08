import { untrack } from 'svelte';

/**
 * Re-initialize dialog form state whenever the dialog opens. This must be an
 * `$effect.pre`: bits-ui only calls `onOpenChange` for its own internal
 * open/close (trigger, Escape), not when `open` is set programmatically via
 * `bind:open` — which is the only way these dialogs are opened — so syncing
 * there never runs. `sync` runs untracked so writes to form state can't
 * re-trigger the effect.
 */
export function useDialogReset(open: () => boolean, sync: () => void): void {
  $effect.pre(() => {
    if (open()) untrack(sync);
  });
}
