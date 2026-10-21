/**
 * Per-account reconcile draft in localStorage. Marks are saved explicitly,
 * but the typed ending balance (and a moved statement date) would otherwise
 * vanish on any reload or back navigation.
 */

export interface ReconcileDraft {
  endingBalance: string;
  statementDate: string;
}

function draftKey(accountGuid: string): string {
  return `reconcile-draft:${accountGuid}`;
}

export function readReconcileDraft(accountGuid: string): ReconcileDraft | null {
  try {
    const raw = localStorage.getItem(draftKey(accountGuid));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<ReconcileDraft>;
    if (typeof parsed.endingBalance !== 'string') return null;
    return {
      endingBalance: parsed.endingBalance,
      statementDate: typeof parsed.statementDate === 'string' ? parsed.statementDate : ''
    };
  } catch {
    return null;
  }
}

export function writeReconcileDraft(accountGuid: string, draft: ReconcileDraft): void {
  localStorage.setItem(draftKey(accountGuid), JSON.stringify(draft));
}

export function removeReconcileDraft(accountGuid: string): void {
  localStorage.removeItem(draftKey(accountGuid));
}
