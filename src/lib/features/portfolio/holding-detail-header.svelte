<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Pencil, RefreshCw } from '@lucide/svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import type { PortfolioHolding } from '$lib/tauri';

  let {
    holding,
    name,
    refreshing,
    onRefresh,
    onEditSymbol,
    children = undefined
  }: {
    holding: PortfolioHolding;
    name: string;
    refreshing: boolean;
    onRefresh: () => void;
    onEditSymbol: () => void;
    children?: Snippet;
  } = $props();

  let positionLabel = $derived(
    holding.shares !== 0 ? null : holding.accounts.length > 0 ? 'Closed' : 'Not held'
  );
</script>

<PageHeader title={name}>
  {#snippet description()}
    {holding.mnemonic} · {holding.namespace}{#if positionLabel}
      · {positionLabel}{/if}
  {/snippet}
  {#if children}
    {@render children()}
  {/if}
  <Button variant="outline" onclick={onRefresh} disabled={refreshing}>
    <RefreshCw aria-hidden="true" class="size-4 {refreshing ? 'animate-spin' : ''}" />
    {refreshing ? 'Refreshing…' : 'Refresh prices'}
  </Button>
  <Button
    variant="outline"
    onclick={onEditSymbol}
    aria-label="Edit Yahoo symbol"
    title="Edit Yahoo symbol"
  >
    <Pencil aria-hidden="true" class="size-4" />
    Edit symbol
  </Button>
</PageHeader>
