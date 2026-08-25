<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { TrendingUp, Columns3 } from '@lucide/svelte';
  import type { PortfolioHolding } from '$lib/tauri';
  import { STALE_PRICE_DAYS, priceStaleDays } from './price-utils';
  import { handleRowClick, gotoHolding } from '$lib/utils/navigation';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import {
    formatDate,
    signColor,
    fmtPct,
    formatSignedCurrency,
    formatSessionCurrency
  } from '$lib/utils/format.svelte';

  type SortKey =
    'name' | 'shares' | 'avgCost' | 'price' | 'marketValue' | 'costBasis' | 'gainLoss' | 'alloc';

  type ColumnId = Exclude<SortKey, 'name'>;

  const COLUMNS: { id: ColumnId; label: string }[] = [
    { id: 'shares', label: 'Shares' },
    { id: 'avgCost', label: 'Avg Cost' },
    { id: 'price', label: 'Price' },
    { id: 'marketValue', label: 'Market Value' },
    { id: 'costBasis', label: 'Cost Basis' },
    { id: 'gainLoss', label: 'Gain / Loss' },
    { id: 'alloc', label: 'Alloc' }
  ];

  const DEFAULT_COLUMNS: ColumnId[] = ['shares', 'price', 'marketValue', 'gainLoss', 'alloc'];

  let {
    holdings = [] as PortfolioHolding[],
    currency,
    loading = false
  }: {
    holdings: PortfolioHolding[];
    currency: string;
    loading?: boolean;
  } = $props();

  let sortBy = $state<SortKey | null>(null);
  let sortDir = $state<'asc' | 'desc'>('desc');

  let visibleColumns = $state<ColumnId[]>([...DEFAULT_COLUMNS]);

  let queryInput = $state('');
  let query = useDebouncedSearch(
    () => queryInput,
    (v) => v.trim().toLowerCase()
  );

  let filteredHoldings = $derived.by(() => {
    const q = query.value;
    if (!q) return holdings;
    return holdings.filter(
      (h) => (h.fullName ?? '').toLowerCase().includes(q) || h.mnemonic.toLowerCase().includes(q)
    );
  });

  let skeletonCols = $derived(visibleColumns.length + 1);

  function isVisible(id: ColumnId): boolean {
    return visibleColumns.includes(id);
  }

  function setVisible(id: ColumnId, checked: boolean) {
    if (checked) {
      if (!visibleColumns.includes(id)) visibleColumns = [...visibleColumns, id];
    } else {
      visibleColumns = visibleColumns.filter((c) => c !== id);
      // Don't leave an invisible sort in effect.
      if (sortBy === id) sortBy = null;
    }
  }

  function resetColumns() {
    visibleColumns = [...DEFAULT_COLUMNS];
  }

  let sortedHoldings = $derived.by(() => {
    if (!sortBy) return filteredHoldings;
    const dir = sortDir === 'asc' ? 1 : -1;
    return [...filteredHoldings].sort((a, b) => {
      // Nulls (missing data) always sort to the bottom.
      const nullCmp = (va: number | null, vb: number | null) =>
        va === null && vb === null ? 0 : va === null ? 1 : vb === null ? -1 : 0;
      let cmp = 0;
      switch (sortBy) {
        case 'name':
          cmp = (a.fullName ?? a.mnemonic).localeCompare(b.fullName ?? b.mnemonic);
          break;
        case 'shares':
          cmp = a.shares - b.shares;
          break;
        case 'avgCost':
          cmp = a.avgCostPerShare - b.avgCostPerShare;
          break;
        case 'price':
          cmp = nullCmp(a.price, b.price) || (a.price ?? 0) - (b.price ?? 0);
          break;
        case 'marketValue':
          cmp = a.marketValue - b.marketValue;
          break;
        case 'costBasis':
          cmp = a.costBasis - b.costBasis;
          break;
        case 'gainLoss':
          cmp = a.gainLoss - b.gainLoss;
          break;
        case 'alloc':
          cmp = a.allocationPct - b.allocationPct;
          break;
      }
      return cmp * dir;
    });
  });

  function toggleSort(key: SortKey) {
    if (sortBy === key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortBy = key;
      sortDir = key === 'name' ? 'asc' : 'desc';
    }
  }

  const fmt = formatSessionCurrency;
</script>

{#if loading}
  <TableSkeleton cols={skeletonCols} rows={4} />
{:else if holdings.length === 0}
  <EmptyState card title="No holdings" description="No securities found in your GnuCash file.">
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <!-- One wrapper so the page-level flex gap doesn't open up between toolbar and table. -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center justify-between gap-2">
      <SearchInput
        id="holdings-search"
        label="Search holdings"
        placeholder="Search name or ticker..."
        bind:value={queryInput}
        class="w-60"
      />
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button {...props} variant="outline" size="sm">
              <Columns3 aria-hidden="true" class="text-muted-foreground" />
              Columns
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end">
          {#each COLUMNS as col (col.id)}
            <DropdownMenu.CheckboxItem
              checked={isVisible(col.id)}
              onCheckedChange={(checked) => setVisible(col.id, checked)}
              closeOnSelect={false}
            >
              {col.label}
            </DropdownMenu.CheckboxItem>
          {/each}
          <DropdownMenu.Separator />
          <DropdownMenu.Item onclick={resetColumns}>Reset to default</DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <SortHead
            label="Holding"
            active={sortBy === 'name'}
            dir={sortDir}
            onclick={() => toggleSort('name')}
          />
          {#if isVisible('shares')}
            <SortHead
              label="Shares"
              active={sortBy === 'shares'}
              dir={sortDir}
              onclick={() => toggleSort('shares')}
              class="text-right"
            />
          {/if}
          {#if isVisible('avgCost')}
            <SortHead
              label="Avg Cost"
              active={sortBy === 'avgCost'}
              dir={sortDir}
              onclick={() => toggleSort('avgCost')}
              class="text-right"
            />
          {/if}
          {#if isVisible('price')}
            <SortHead
              label="Price"
              active={sortBy === 'price'}
              dir={sortDir}
              onclick={() => toggleSort('price')}
              class="text-right"
            />
          {/if}
          {#if isVisible('marketValue')}
            <SortHead
              label="Market Value"
              active={sortBy === 'marketValue'}
              dir={sortDir}
              onclick={() => toggleSort('marketValue')}
              class="text-right"
            />
          {/if}
          {#if isVisible('costBasis')}
            <SortHead
              label="Cost Basis"
              active={sortBy === 'costBasis'}
              dir={sortDir}
              onclick={() => toggleSort('costBasis')}
              class="text-right"
            />
          {/if}
          {#if isVisible('gainLoss')}
            <SortHead
              label="Gain / Loss"
              active={sortBy === 'gainLoss'}
              dir={sortDir}
              onclick={() => toggleSort('gainLoss')}
              class="text-right"
            />
          {/if}
          {#if isVisible('alloc')}
            <SortHead
              label="Alloc"
              active={sortBy === 'alloc'}
              dir={sortDir}
              onclick={() => toggleSort('alloc')}
              class="text-right"
            />
          {/if}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#if filteredHoldings.length === 0}
          <Table.Row>
            <Table.Cell class="h-24 text-center text-muted-foreground" colspan={skeletonCols}>
              No holdings match “{query}”.
            </Table.Cell>
          </Table.Row>
        {:else}
          {#each sortedHoldings as h (h.commodityGuid)}
            {@const isClosed = h.shares === 0 && h.accounts.length > 0}
            {@const isWatchOnly = h.shares === 0 && h.accounts.length === 0}
            <Table.Row
              class="cursor-pointer {isClosed || isWatchOnly ? 'text-muted-foreground' : ''}"
              onclick={(e) => handleRowClick(e, () => gotoHolding(h.commodityGuid))}
            >
              <Table.Cell class="py-2">
                <!-- Real link for keyboard/AT users; the row click stays as a
                 convenience target for pointer input. -->
                <a
                  href={resolve('/portfolio/[holding]', { holding: h.commodityGuid })}
                  class="flex flex-col gap-0.5 focus-visible:outline-2 focus-visible:outline-ring"
                >
                  <span class="flex items-center gap-2 font-medium">
                    <span class="truncate">{h.fullName ?? h.mnemonic}</span>
                    {#if isClosed}
                      <Badge variant="outline" class="shrink-0 text-[10px] leading-none"
                        >Closed</Badge
                      >
                    {:else if isWatchOnly}
                      <Badge variant="outline" class="shrink-0 text-[10px] leading-none"
                        >Not held</Badge
                      >
                    {/if}
                  </span>
                  <span class="truncate text-xs text-muted-foreground">{h.mnemonic}</span>
                </a>
              </Table.Cell>
              {#if isVisible('shares')}
                <Table.Cell class="py-2.5 text-right tabular-nums">{h.shares.toFixed(2)}</Table.Cell
                >
              {/if}
              {#if isVisible('avgCost')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {h.shares > 0 ? fmt(h.avgCostPerShare) : '\u2014'}
                </Table.Cell>
              {/if}
              {#if isVisible('price')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if h.price !== null}
                    {@const stale =
                      h.priceDate !== null && priceStaleDays(h.priceDate) > STALE_PRICE_DAYS}
                    <span
                      class={stale ? 'text-warning' : ''}
                      title={h.priceDate === null ? undefined : formatDate(h.priceDate)}
                    >
                      {fmt(h.price)}
                    </span>
                  {:else}
                    —
                  {/if}
                </Table.Cell>
              {/if}
              {#if isVisible('marketValue')}
                <Table.Cell class="py-2.5 text-right font-medium tabular-nums">
                  {fmt(h.marketValue)}
                </Table.Cell>
              {/if}
              {#if isVisible('costBasis')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {fmt(h.costBasis)}
                </Table.Cell>
              {/if}
              {#if isVisible('gainLoss')}
                <Table.Cell class="py-2.5 text-right tabular-nums {signColor(h.gainLoss)}">
                  {formatSignedCurrency(h.gainLoss, currency)}
                  <span class="text-xs"> ({fmtPct(h.gainLossPct)})</span>
                </Table.Cell>
              {/if}
              {#if isVisible('alloc')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {h.allocationPct.toFixed(1)}%
                </Table.Cell>
              {/if}
            </Table.Row>
          {/each}
        {/if}
      </Table.Body>
    </TableShell>
  </div>
{/if}
