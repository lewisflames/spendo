<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import { CircleCheck, Scale, Target, Wallet } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import { EMPTY_DASH, formatCurrency } from '$lib/utils/format.svelte';
  import { fromUnits, type Units } from '$lib/utils/units';

  let {
    currency,
    startingBalance,
    cleared,
    target,
    difference,
    balanced,
    onFindDifference
  }: {
    currency: string;
    startingBalance: Units;
    cleared: Units;
    target: Units;
    /** `null` while the ending balance is empty or invalid. */
    difference: Units | null;
    balanced: boolean;
    onFindDifference: () => void;
  } = $props();

  const fmt = (units: Units) => formatCurrency(fromUnits(units), currency);
  // No color without a usable ending balance. Any non-zero difference is a
  // mismatch, so it reads as a warning regardless of sign.
  let differenceClass = $derived(
    difference === null ? '' : balanced ? 'text-success' : 'text-warning'
  );
</script>

<div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
  <KpiCard title="Starting (reconciled)" value={fmt(startingBalance)}>
    {#snippet icon()}
      <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </KpiCard>
  <KpiCard title="Cleared" value={fmt(cleared)}>
    {#snippet icon()}
      <CircleCheck aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </KpiCard>
  <KpiCard title="Target" value={fmt(target)}>
    {#snippet icon()}
      <Target aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </KpiCard>
  <KpiCard title="Difference" valueClass={differenceClass}>
    {#snippet icon()}
      <Scale aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      <span aria-live="polite">
        {#if difference === null}
          {EMPTY_DASH}
        {:else if !balanced}
          <button
            type="button"
            class="rounded-sm underline decoration-dotted underline-offset-4 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
            {@attach tooltip('Show the candidates matching this amount')}
            onclick={onFindDifference}
          >
            {fmt(difference)}
          </button>
        {:else}
          {fmt(difference)}
        {/if}
      </span>
    {/snippet}
  </KpiCard>
</div>
