<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Pencil, RefreshCw } from '@lucide/svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import type { PortfolioHolding } from '$lib/tauri';

  let {
    holding,
    name,
    refreshing,
    onRefresh,
    onEditSymbol
  }: {
    holding: PortfolioHolding;
    name: string;
    refreshing: boolean;
    onRefresh: () => void;
    onEditSymbol: () => void;
  } = $props();

  let positionLabel = $derived(
    holding.shares !== 0 ? null : holding.accounts.length > 0 ? 'Closed' : 'Not held'
  );
</script>

<PageHeader title={name}>
  {#snippet description()}
    {holding.mnemonic} · {holding.namespace}{#if positionLabel}· {positionLabel}{/if}{#if holding.priceDate}{/if}
  {/snippet}
  <div class="flex items-center gap-2">
    <Button onclick={onRefresh} disabled={refreshing}>
      <RefreshCw aria-hidden="true" class="size-3.5 {refreshing ? 'animate-spin' : ''}" />
      {refreshing ? 'Refreshing...' : 'Refresh Prices'}
    </Button>
    <Button
      variant="outline"
      onclick={onEditSymbol}
      aria-label="Edit Yahoo symbol"
      title="Edit Yahoo symbol"
    >
      <Pencil aria-hidden="true" />
      Edit Symbol
    </Button>
  </div>
</PageHeader>
