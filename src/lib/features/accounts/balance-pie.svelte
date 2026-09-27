<script lang="ts">
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';
  import { clampZero, formatAllocPct } from '$lib/utils/format.svelte';
  import type { AccountNode, ParentAccountInfo } from '$lib/tauri';
  import AllocationPie from './allocation-pie.svelte';

  let {
    node,
    parent,
    currency,
    loading
  }: {
    node: AccountNode | null;
    parent: ParentAccountInfo | null;
    currency?: string;
    loading: boolean;
  } = $props();

  let accountBalance = $derived(Math.abs(clampZero(node?.balance ?? 0)));

  let pieData = $derived.by(() => {
    if (!node) return [];
    return node.children
      .filter((c) => Math.abs(clampZero(c.balance)) >= 0.005)
      .map((c) => {
        const abs = Math.abs(clampZero(c.balance));
        return {
          key: c.guid,
          label: c.name,
          value: abs,
          color: TYPE_COLOR[c.accountType] ?? 'var(--color-muted-foreground)',
          sharePct: accountBalance > 0 ? (abs / accountBalance) * 100 : null
        };
      });
  });

  let parentSharePct = $derived.by((): number | null => {
    if (!parent || Math.abs(clampZero(parent.balance)) < 0.005) return null;
    return (accountBalance / Math.abs(clampZero(parent.balance))) * 100;
  });

  let parentLabel = $derived(parent?.name ?? '');
  let selfLabel = $derived(node?.name ?? '');
  // Top-level accounts have no parent: show 100% relative to itself so the
  // donut hole is never empty for a meaningful balance.
  let centerLabel = $derived(
    parentSharePct !== null
      ? `of ${parentLabel}`
      : accountBalance >= 0.005 && node
        ? `of ${selfLabel}`
        : null
  );
  let centerValue = $derived(
    parentSharePct !== null
      ? formatAllocPct(parentSharePct)
      : accountBalance >= 0.005 && node
        ? formatAllocPct(100)
        : null
  );
</script>

<AllocationPie
  data={pieData}
  title="Balance Composition"
  description="How the pieces add up, by type."
  {currency}
  {centerLabel}
  {centerValue}
  {loading}
  emptyMessage="Nothing broken down yet."
/>
