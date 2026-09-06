<script lang="ts">
  import { Wallet, TrendingUp, TrendingDown } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import FutureAsOfNote from '$lib/components/shared/future-as-of-note.svelte';
  import { formatSessionCurrency as fmt, formatDate } from '$lib/utils/format.svelte';
  import type { AccountSummary } from '$lib/tauri';

  let {
    summary,
    currentBalance,
    isToday,
    asOf
  }: {
    summary: AccountSummary;
    currentBalance: number;
    isToday: boolean;
    asOf: string;
  } = $props();
</script>

<div class="grid grid-cols-1 gap-6 sm:grid-cols-3">
  <KpiCard title="Total Balance" value={fmt(currentBalance)}>
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
{#if !isToday}
  <p class="text-xs text-muted-foreground">
    Balances as of {formatDate(asOf)} — the transaction list below keeps its own From/To filters.
  </p>
{/if}
<FutureAsOfNote {asOf} />
