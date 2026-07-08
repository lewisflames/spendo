import type { BadgeVariant } from '$lib/components/ui/badge';

export interface ReconcileStatus {
  label: string;
  variant: BadgeVariant;
}

/** GnuCash reconcile state ('n'/'c'/'y') as a pill variant + label. */
export function statusFor(reconcileState?: string | null): ReconcileStatus {
  if (reconcileState === 'y') return { label: 'Reconciled', variant: 'success' };
  if (reconcileState === 'c') return { label: 'Cleared', variant: 'warning' };
  return { label: 'New', variant: 'secondary' };
}
