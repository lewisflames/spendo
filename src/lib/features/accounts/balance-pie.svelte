<script lang="ts">
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';
  import { clampZero } from '$lib/utils/format.svelte';
  import type { AccountNode, ParentAccountInfo } from '$lib/tauri';
  import AllocationPie from './allocation-pie.svelte';

  let {
    node,
    parent,
    loading
  }: {
    node: AccountNode | null;
    parent: ParentAccountInfo | null;
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
          label: c.description ?? c.name,
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

  let parentLabel = $derived(parent?.description ?? parent?.name ?? '');
  let centerLabel = $derived(parentSharePct !== null ? `of ${parentLabel}` : null);
  let centerValue = $derived(parentSharePct !== null ? `${parentSharePct.toFixed(1)}%` : null);
</script>

<AllocationPie
  data={pieData}
  title="Balance Composition"
  description="Sub-account balances by type."
  {centerLabel}
  {centerValue}
  {loading}
  emptyMessage="No sub-account balances found."
/>
