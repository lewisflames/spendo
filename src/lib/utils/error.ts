import type { BackendError, BackendErrorKind } from '$lib/tauri';

const KINDS: readonly string[] = [
  'notFound',
  'validation',
  'io',
  'db',
  'internal'
] satisfies BackendErrorKind[];

/** Whether `e` is a command rejection (`{ kind, message }` from Rust `AppError`). */
export function isBackendError(e: unknown): e is BackendError {
  if (!e || typeof e !== 'object') return false;
  const { kind, message } = e as { kind?: unknown; message?: unknown };
  return typeof kind === 'string' && KINDS.includes(kind) && typeof message === 'string';
}

export function errMsg(e: unknown, fallback: string): string {
  if (isBackendError(e)) return e.message;
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return fallback;
}

/** Backend error kind of a failure, or `null` for anything else. */
export function errKind(e: unknown): BackendErrorKind | null {
  return isBackendError(e) ? e.kind : null;
}
