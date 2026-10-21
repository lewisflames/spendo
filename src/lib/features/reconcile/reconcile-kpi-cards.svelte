<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import { CircleCheck, Scale, Target, Wallet } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import { EMPTY_DASH, clampZero, formatCurrency } from '$lib/utils/format.svelte';

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
    startingBalance: number;
    cleared: number;
    target: number;
    difference: number;
    balanced: boolean;
    onFindDifference: () => void;
  } = $props();

  // Snap float dust (e.g. -2.9e-12 from starting + cleared) so a balanced statement shows US$0.00, not -US$0.00.
  const fmt = (n: number) => formatCurrency(clampZero(n), currency);
  // `difference` is NaN while the ending balance is empty or invalid; keep the
  // color class off in that case. Any non-zero difference is a mismatch, so it
  // reads as a warning regardless of sign.
  let differenceClass = $derived(
    !Number.isFinite(difference) ? '' : balanced ? 'text-success' : 'text-warning'
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
        {#if !Number.isFinite(difference)}
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
