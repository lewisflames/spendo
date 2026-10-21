<script lang="ts">
  import { Wallet, TrendingUp, TrendingDown } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import { formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import type { AccountSummary } from '$lib/tauri';

  let { summary, currentBalance }: { summary: AccountSummary; currentBalance: number } = $props();
</script>

<div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @3xl:grid-cols-3">
  <KpiCard title="Total balance" value={fmt(currentBalance)}>
    {#snippet icon()}
      <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </KpiCard>
  <KpiCard title="Income" value={fmt(summary.totalIncome)} valueClass="text-success">
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="size-4 text-success" />
    {/snippet}
  </KpiCard>
  <KpiCard title="Expenses" value={fmt(summary.totalExpenses)} valueClass="text-destructive">
    {#snippet icon()}
      <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
    {/snippet}
  </KpiCard>
</div>
