export function errMsg(e: unknown, fallback: string): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return fallback;
}

/**
 * Backend "missing entity" errors surface as `NotFound("… not found …")` or
 * `"No budget for …"`. Map those to `EmptyState` instead of `ErrorCard` so
 * deleted / year-less entities share one not-found pattern.
 */
export function isNotFoundMessage(message: string | null | undefined): boolean {
  if (!message) return false;
  return /not found|no budget for/i.test(message);
}
