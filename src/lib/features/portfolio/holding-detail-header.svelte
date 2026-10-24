<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { HandCoins, Pencil, RefreshCw } from '@lucide/svelte';
  import PageHeader, { type Breadcrumb } from '$lib/components/layout/page-header.svelte';
  import OverflowMenu from '$lib/components/shared/overflow-menu.svelte';
  import type { PortfolioHolding } from '$lib/tauri';

  let {
    holding,
    name,
    refreshing,
    onRefresh,
    onEditSymbol,
    onEditDividends,
    breadcrumbs = [],
    children = undefined
  }: {
    holding: PortfolioHolding;
    name: string;
    refreshing: boolean;
    onRefresh: () => void;
    onEditSymbol: () => void;
    onEditDividends: () => void;
    breadcrumbs?: Breadcrumb[];
    children?: Snippet;
  } = $props();

  let positionLabel = $derived(
    holding.shares !== 0 ? null : holding.accounts.length > 0 ? 'Closed' : 'Not held'
  );
</script>

<PageHeader title={name} {breadcrumbs}>
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
  <OverflowMenu
    actions={[
      { label: 'Edit Yahoo symbol', icon: Pencil, onSelect: onEditSymbol },
      { label: 'Dividend accounts', icon: HandCoins, onSelect: onEditDividends }
    ]}
  />
</PageHeader>
