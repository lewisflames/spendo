<script lang="ts">
  import { cn } from '$lib/utils';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import {
    formatSessionCurrency as fmt,
    formatSignedCurrency,
    signColor
  } from '$lib/utils/format.svelte';
  import type { BudgetYearSummary } from '$lib/tauri';

  let {
    summary,
    annualTarget,
    currency
  }: {
    summary: BudgetYearSummary;
    annualTarget: number;
    currency: string;
  } = $props();
</script>

<!-- Yearly summary -->
<div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
  <KpiCard title="Annual target" value={fmt(annualTarget)} valueClass="text-chart-1" />
  <KpiCard
    title="Carry-in (Jan 1)"
    value={formatSignedCurrency(summary.rolloverBalance, currency)}
    valueClass={cn(summary.rolloverBalance < 0 ? 'text-destructive' : 'text-success')}
  />
  <KpiCard
    title="Spent (YTD)"
    value={fmt(summary.actualAmount)}
    valueClass={cn(
      summary.actualAmount > summary.effectiveTotal ? 'text-destructive' : 'text-success'
    )}
  />
  <KpiCard
    title="Remaining"
    value={formatSignedCurrency(summary.remaining, currency)}
    valueClass={signColor(summary.remaining)}
  />
</div>
