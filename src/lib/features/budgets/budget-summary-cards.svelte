<script lang="ts">
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import {
    formatSessionCurrency as fmt,
    formatSignedCurrency,
    clampZero,
    signColor
  } from '$lib/utils/format.svelte';
  import type { BudgetYearSummary } from '$lib/tauri';
  import { fromUnits, subUnits, type Units } from '$lib/utils/units';

  let {
    summary,
    annualTarget,
    currency
  }: {
    summary: BudgetYearSummary;
    annualTarget: Units;
    currency: string;
  } = $props();

  let carry = $derived(fromUnits(summary.rolloverBalance));
  let remaining = $derived(fromUnits(summary.remaining));
</script>

<!-- Yearly summary -->
<div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
  <KpiCard title="Annual target" value={fmt(clampZero(fromUnits(annualTarget)))} />
  <KpiCard
    title="Carry-in (Jan 1)"
    value={formatSignedCurrency(clampZero(carry), currency)}
    valueClass={signColor(clampZero(carry))}
  />
  <KpiCard
    title="Spent (YTD)"
    value={fmt(clampZero(fromUnits(summary.actualAmount)))}
    valueClass={signColor(subUnits(summary.effectiveTotal, summary.actualAmount))}
  />
  <KpiCard
    title="Remaining"
    value={formatSignedCurrency(clampZero(remaining), currency)}
    valueClass={signColor(clampZero(remaining))}
  />
</div>
