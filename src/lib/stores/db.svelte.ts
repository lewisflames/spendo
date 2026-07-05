import type { GnucashSummary } from '$lib/server/gnucash-parser';

interface DbSession {
  summary: GnucashSummary;
  filename: string | null;
}

export const dbSession = $state<{ value: DbSession | null }>({ value: null });

export function clearSession() {
  dbSession.value = null;
}
