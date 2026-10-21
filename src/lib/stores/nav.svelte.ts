import { fetchNavBadges, type NavBadges } from '$lib/tauri';

export const navBadges = $state<{ value: NavBadges }>({
  value: { dueSchedules: 0, unclearedTransactions: 0 }
});

let requestSeq = 0;

/** Refresh the sidebar badges. Failures keep the previous
 * values: the shell must never break on a transient query error. Responses
 * from an older refresh are dropped so a fast book switch cannot overwrite
 * the new book’s nav. */
export async function refreshNav(): Promise<void> {
  const seq = ++requestSeq;
  try {
    const badges = await fetchNavBadges();
    if (seq !== requestSeq) return;
    navBadges.value = badges;
  } catch {
    // keep previous values — a later navigation retries
  }
}

/**
 * Drop cached badges on book switches so the sidebar can’t briefly show the
 * previous book’s counts. Bumps the request
 * sequence so an in-flight refresh for the old book is discarded.
 */
export function resetNav(): void {
  requestSeq += 1;
  navBadges.value = { dueSchedules: 0, unclearedTransactions: 0 };
}
