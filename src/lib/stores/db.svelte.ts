import { fetchHomeData, type DataSummary } from '$lib/tauri';

interface DbSession {
  summary: DataSummary;
}

export const dbSession = $state<{ value: DbSession | null }>({ value: null });

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
  });
  return inflight;
}

export function clearSession() {
  dbSession.value = null;
}
