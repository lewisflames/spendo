<script lang="ts">
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import {
    formatSessionCurrency as fmt,
    formatSignedCurrency,
    clampZero,
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
  <KpiCard title="Annual Target" value={fmt(clampZero(annualTarget))} />
  <KpiCard
    title="Carry-in (Jan 1)"
    value={formatSignedCurrency(clampZero(summary.rolloverBalance), currency)}
    valueClass={signColor(clampZero(summary.rolloverBalance))}
  />
  <KpiCard
    title="Spent (YTD)"
    value={fmt(clampZero(summary.actualAmount))}
    valueClass={signColor(clampZero(summary.effectiveTotal - summary.actualAmount))}
  />
  <KpiCard
    title="Remaining"
    value={formatSignedCurrency(clampZero(summary.remaining), currency)}
    valueClass={signColor(clampZero(summary.remaining))}
  />
</div>
