<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Card from '$lib/components/ui/card';
  import Amount from '$lib/components/shared/amount.svelte';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import { HandCoins } from '@lucide/svelte';
  import { plural } from '$lib/utils/format.svelte';
  import type { TaxSummary } from '$lib/tauri';

  let { summary }: { summary: TaxSummary } = $props();

  let r = $derived(summary.realized);
  // Query strings can't go through `resolve()`.
  let gainsHref = $derived(`${resolve('/portfolio/gains')}?year=${summary.year}`);
</script>

<Card.Root>
  <IconCardHeader
    title="Realized gains"
    description={r.disposals > 0
      ? `${plural(r.disposals, 'lot')} sold in ${summary.year}`
      : `Nothing sold in ${summary.year}`}
  >
    {#snippet icon()}
      <HandCoins aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3 text-sm">
    <dl class="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1.5">
      <dt class="text-muted-foreground">Gains</dt>
      <dd class="text-right"><Amount value={r.gains} /></dd>
      <dt class="text-muted-foreground">Losses</dt>
      <dd class="text-right"><Amount value={r.losses} /></dd>
      <dt class="font-medium">Net</dt>
      <dd class="text-right"><Amount value={r.net} tone weight="semibold" /></dd>
    </dl>
    {#if summary.costBasisMethod === 'average'}
      <p class="text-xs text-muted-foreground">
        Lots use average cost. Many tax authorities, Spain’s among them, require first in, first
        out: switch it under Settings → Reporting.
      </p>
    {/if}
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
    <a href={gainsHref} class="text-xs font-medium underline-offset-2 hover:underline">
      See every sale
    </a>
  </Card.Content>
</Card.Root>
