import { fetchHomeData, fetchActiveBook, type DataSummary } from '$lib/tauri';

interface DbSession {
  summary: DataSummary;
}

export const dbSession = $state<{ value: DbSession | null }>({ value: null });

/** Active `.spendo` file, or `null` while book-less (fresh install). */
export const activeBookPath = $state<{ value: string | null }>({ value: null });

/**
 * True only during the first bootstrap fetch. Stays false for later
 * force-refreshes (home revalidates on every visit), so shell/redirect logic
 * can tell a cold start apart from a refresh without flapping.
 */
export const dbBootstrapping = $state<{ value: boolean }>({ value: true });

/**
 * Single write path for the session so every flow (bootstrap, open/new/save,
 * import, reimport) goes through one place. Pass `null` to return to the empty
 * state. Bumps the sequence so in-flight refreshes from the previous book are
 * discarded instead of overwriting this state when they resolve.
 */
export function setDbSession(summary: DataSummary | null) {
  sessionSeq += 1;
  dbSession.value = summary ? { summary } : null;
}

let bootstrapped = false;
let inflight: Promise<void> | null = null;
/** Bumped by every `setDbSession`; stale async results compare against it. */
let sessionSeq = 0;

/**
 * Populate `dbSession` from the backend. Called by the layout once at startup
 * so deep-linked pages (e.g. Settings) see real state, and by the home page
 * with `force` to refresh on every visit.
 *
 * Concurrent callers share one in-flight fetch (layout bootstrap + home
 * refresh on first paint); refreshes never touch `dbBootstrapping`.
 */
export function ensureDbSessionLoaded(force = false): Promise<void> {
  if (inflight) {
    return inflight;
  }
  if (!force && bootstrapped) {
    return Promise.resolve();
  }
  const isBootstrap = !bootstrapped;
  bootstrapped = true;
  if (isBootstrap) {
    dbBootstrapping.value = true;
  }
  const seq = sessionSeq;
  const p = fetchHomeData()
    .then((result) => {
      // A book switch or close landed while this fetch was in flight; its
      // summary belongs to the previous book and must not overwrite the new
      // session (or revive a closed one).
      if (seq !== sessionSeq) return;
      // A summary exists whenever a book is open, even an empty one — the
      // shell must stay so pages can show their empty states.
      setDbSession(result.summary);
    })
    .catch((e) => {
      // A failed bootstrap must not stick: allow a later call to retry so a
      // transient startup error doesn’t leave the session null forever.
      bootstrapped = false;
      throw e;
    });
  const q = p.finally(() => {
    if (inflight === q) {
      inflight = null;
    }
    if (isBootstrap) {
      dbBootstrapping.value = false;
    }
  });
  inflight = q;
  return q;
}

/**
 * Fire-and-forget session refresh after a mutation that returns no summary
 * (account edits, reconcile saves). The refreshed summary makes the layout
 * effect reload nav roots and badges; a failure is ignored because the next
 * navigation retries.
 */
export function refreshSession() {
  void ensureDbSessionLoaded(true).catch(() => {});
}

/** Refresh `activeBookPath` from the backend. Failures keep the previous
 * value; pages that need the path surface their own errors. */
export async function loadActiveBook(): Promise<void> {
  const seq = sessionSeq;
  try {
    const path = (await fetchActiveBook()).path;
    if (seq !== sessionSeq) return;
    activeBookPath.value = path;
  } catch {
    // ignore — a later navigation retries
  }
}

export function setActiveBookPath(path: string | null) {
  activeBookPath.value = path;
}
