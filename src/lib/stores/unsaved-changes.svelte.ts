/**
 * Store convention: `$state()` wrapped in `{ value }` like every other store.
 *
 * One mounted form registers itself here so navigation and book actions can
 * ask before edits are abandoned. The form owns the actual dialog; this store
 * only routes the question and remembers whether anything is at stake.
 */
export const unsavedChanges = $state({
  value: {
    dirty: false,
    /** Set once a caller confirmed leaving, so navigation doesn’t ask again. */
    waived: false,
    /** Shows the form’s discard dialog and resolves true when leaving is OK. */
    confirmLeave: null as (() => Promise<boolean>) | null
  }
});

/**
 * Ask the mounted form (if any) before abandoning edits. A confirmed discard
 * waives the guard so the follow-up navigation or book switch isn’t blocked a
 * second time; callers re-arm it with {@link clearDiscardWaiver} when the
 * switch didn’t end up navigating away.
 */
export async function confirmDiscardIfDirty(): Promise<boolean> {
  if (!unsavedChanges.value.dirty) return true;
  const confirmLeave = unsavedChanges.value.confirmLeave;
  const leave = confirmLeave ? await confirmLeave() : true;
  if (leave) unsavedChanges.value.waived = true;
  return leave;
}

/** Re-arm the guard after a switch that failed or was cancelled. */
export function clearDiscardWaiver(): void {
  unsavedChanges.value.waived = false;
}
