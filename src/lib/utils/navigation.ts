import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

/** Navigate to a transaction detail page. */
export function openTransaction(guid: string): Promise<void> {
  return goto(resolve('/transactions/[id]', { id: guid }));
}

/** Navigate to an account detail page. */
export function openAccount(guid: string): Promise<void> {
  return goto(resolve('/accounts/[guid]', { guid }));
}

/** Navigate to a holding detail page. */
export function openHolding(commodityGuid: string): Promise<void> {
  return goto(resolve('/portfolio/[holding]', { holding: commodityGuid }));
}

/** Navigate to an account's budget page, preserving the selected year. */
export function openBudget(accountGuid: string, year?: number): Promise<void> {
  const base = resolve('/budgets/[accountGuid]', { accountGuid });
  // Query-only year suffix on top of a resolved route.
  // eslint-disable-next-line svelte/no-navigation-without-resolve
  return goto(year !== undefined ? `${base}?year=${year}` : base);
}

/** Navigate to an account's reconciliation session. */
export function openReconcile(guid: string): Promise<void> {
  return goto(resolve('/reconcile/[guid]', { guid }));
}

/**
 * Row-click navigation guard shared by every clickable table row. Ignores
 * clicks with modifier keys (open in new tab etc.) and clicks on interactive
 * children (links, buttons, inputs) so the inner anchor keeps its native
 * behavior.
 */
function isPlainRowClick(e: MouseEvent): boolean {
  if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) {
    return false;
  }
  const target = e.target as HTMLElement | null;
  return !target?.closest('a, button, input, select, textarea, label');
}

export function handleRowClick(e: MouseEvent, go: () => void): void {
  if (isPlainRowClick(e)) go();
}
