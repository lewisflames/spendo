import { fetchNavBadges, fetchNavRoots, type NavBadges, type NavRoot } from '$lib/tauri';

export const navRoots = $state<{ value: NavRoot[] }>({ value: [] });

export const navBadges = $state<{ value: NavBadges }>({
  value: { dueSchedules: 0, unclearedTransactions: 0 }
});

let requestSeq = 0;

/** Refresh the sidebar's account roots + badges. Failures keep the previous
 * values: the shell must never break on a transient query error. Responses
 * from an older refresh are dropped so a fast book switch cannot overwrite
 * the new book's nav. */
export async function refreshNav(): Promise<void> {
  const seq = ++requestSeq;
  try {
    const [roots, badges] = await Promise.all([fetchNavRoots(), fetchNavBadges()]);
    if (seq !== requestSeq) return;
    navRoots.value = roots;
    navBadges.value = badges;
  } catch {
    // keep previous values — a later navigation retries
  }
}
