<script lang="ts">
  import PacingBar from './pacing-bar.svelte';
  import { formatSessionCurrency as fmt, signColor } from '$lib/utils/format.svelte';
  import type { BudgetYearSummary } from '$lib/tauri';

  let { summary }: { summary: BudgetYearSummary } = $props();
</script>

<!-- Pacing vs expected YTD -->
<div class="rounded-lg border p-4">
  <div class="flex flex-wrap items-baseline justify-between gap-2">
    <h2 class="text-sm font-medium">
      Pacing vs expected YTD
      <span class="font-normal text-muted-foreground">
        ({summary.elapsedMonths}/12 months elapsed)
      </span>
    </h2>
    <p class="text-sm text-muted-foreground tabular-nums">
      {fmt(summary.actualAmount)} of {fmt(summary.expectedYtd)} expected
      {#if summary.elapsedMonths > 0}
        ·
        <span class={signColor(summary.delta)}>
          {#if summary.delta >= 0}
            {fmt(summary.delta)} under
          {:else}
            {fmt(-summary.delta)} over
          {/if}
        </span>
      {/if}
    </p>
  </div>
  {#if summary.elapsedMonths > 0}
    <div class="mt-3 flex items-center gap-3">
      <PacingBar
        pacingPercentage={summary.pacingPercentage}
        actualAmount={summary.actualAmount}
        expectedYtd={summary.expectedYtd}
        delta={summary.delta}
        {fmt}
        size="md"
        tooltip={false}
      />
    </div>
    <p class="mt-2 text-xs text-muted-foreground">
      Full-year progress: {Math.max(0, summary.percentage).toFixed(1)}% of {fmt(
        summary.effectiveTotal
      )} effective total. Carry-in counts in full from January.
    </p>
  {:else}
    <p class="mt-2 text-sm text-muted-foreground">Future year — no pacing yet.</p>
  {/if}
</div>
