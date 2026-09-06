import { fetchHomeData, getActiveBook, type DataSummary } from '$lib/tauri';

interface DbSession {
  summary: DataSummary;
}

export const dbSession = $state<{ value: DbSession | null }>({ value: null });

/** Active `.spendo` file, or `null` while book-less (fresh install). */
export const activeBookPath = $state<{ value: string | null }>({ value: null });

/** True while the first session fetch is in flight; lets pages tell bootstrapping apart from empty. */
export const dbSessionLoading = $state<{ value: boolean }>({ value: true });

let bootstrapped = false;
let inflight: Promise<void> | null = null;

/**
 * Populate `dbSession` from the backend. Called by the layout once at startup
 * so deep-linked pages (e.g. Settings) see real state, and by the home page
 * with `force` to refresh on every visit.
 */
export function ensureDbSessionLoaded(force = false): Promise<void> {
  if (!force && (bootstrapped || inflight)) {
    return inflight ?? Promise.resolve();
  }
  bootstrapped = true;
  dbSessionLoading.value = true;
  const p = fetchHomeData()
    .then((result) => {
      dbSession.value = result.hasData && result.summary ? { summary: result.summary } : null;
    })
    .catch((e) => {
      // A failed bootstrap must not stick: allow a later call to retry so a
      // transient startup error doesn't leave the session null forever.
      bootstrapped = false;
      throw e;
    });
  inflight = p.finally(() => {
    inflight = null;
    dbSessionLoading.value = false;
  });
  return inflight;
}

export function clearSession() {
  dbSession.value = null;
  // The (empty) book stays open — welcome offers New/Open from here.
}

/** Refresh `activeBookPath` from the backend. Failures keep the previous
 * value; pages that need the path surface their own errors. */
export async function loadActiveBook(): Promise<void> {
  try {
    activeBookPath.value = (await getActiveBook()).path;
  } catch {
    // ignore — a later navigation retries
  }
}

export function setActiveBookPath(path: string | null) {
  activeBookPath.value = path;
}
