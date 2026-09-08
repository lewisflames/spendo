<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { resolve } from '$app/paths';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import ToolbarSkeleton from '$lib/components/shared/toolbar-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { TrendingUp } from '@lucide/svelte';
  import type { PortfolioHolding } from '$lib/tauri';
  import { STALE_PRICE_DAYS, priceStaleDays } from './price-utils';
  import { openHolding } from '$lib/utils/navigation';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import {
    formatCurrency,
    formatDate,
    signColor,
    fmtPct,
    formatSignedCurrency,
    formatShares,
    formatAllocPct,
    EMPTY_DASH
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

  let sort = useTableSort<SortKey>({ sortBy: 'marketValue', sortDir: 'desc' }, [
    'shares',
    'avgCost',
    'price',
    'marketValue',
    'costBasis',
    'gainLoss',
    'alloc'
  ]);
  let sortBy = $derived(sort.sortBy);
  let sortDir = $derived(sort.sortDir);

  const cols = useColumnVisibility<ColumnId>(DEFAULT_COLUMNS, 'cols:holdings:v1');
  let visibleColumns = $derived(cols.visibleColumns);

  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);

  let filteredHoldings = $derived.by(() => {
    const q = search.value.trim().toLowerCase();
    if (!q) return holdings;
    return holdings.filter(
      (h) => (h.fullName ?? '').toLowerCase().includes(q) || h.mnemonic.toLowerCase().includes(q)
    );
  });

  let skeletonCols = $derived(visibleColumns.length + 1);

  function isVisible(id: ColumnId): boolean {
    return cols.isVisible(id);
  }

  // Don't leave an invisible sort in effect (default back to market value).
  function handleHideColumn(id: ColumnId) {
    if (sortBy === id) sort.toggle('marketValue');
  }

  let sortedHoldings = $derived.by(() => {
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

  function fmt(n: number): string {
    return formatCurrency(n, currency);
  }
</script>

{#if loading}
  <!-- Same wrapper as the loaded toolbar + table so neither pops in late. -->
  <div class="flex flex-col gap-4">
    <ToolbarSkeleton />
    <TableSkeleton cols={skeletonCols} rows={4} />
  </div>
{:else if holdings.length === 0}
  <EmptyState card title="No holdings" description="No securities found in this book.">
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <!-- One wrapper so the page-level flex gap doesn't open up between toolbar and table. -->
  <div class="flex flex-col gap-4">
    <div class="flex items-center justify-between gap-2">
      <SearchInput
        id="holdings-search"
        label="Search holdings"
        placeholder="Search name or ticker…"
        bind:value={searchInput}
        class="w-60"
      />
      <ColumnsMenu columns={COLUMNS} {cols} onHide={handleHideColumn} />
    </div>
    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <SortHead
            label="Holding"
            active={sortBy === 'name'}
            dir={sortDir}
            onclick={() => sort.toggle('name')}
          />
          {#if isVisible('shares')}
            <SortHead
              label="Shares"
              active={sortBy === 'shares'}
              dir={sortDir}
              onclick={() => sort.toggle('shares')}
              class="text-right"
            />
          {/if}
          {#if isVisible('avgCost')}
            <SortHead
              label="Avg Cost"
              active={sortBy === 'avgCost'}
              dir={sortDir}
              onclick={() => sort.toggle('avgCost')}
              class="text-right"
            />
          {/if}
          {#if isVisible('price')}
            <SortHead
              label="Price"
              active={sortBy === 'price'}
              dir={sortDir}
              onclick={() => sort.toggle('price')}
              class="text-right"
            />
          {/if}
          {#if isVisible('marketValue')}
            <SortHead
              label="Market Value"
              active={sortBy === 'marketValue'}
              dir={sortDir}
              onclick={() => sort.toggle('marketValue')}
              class="text-right"
            />
          {/if}
          {#if isVisible('costBasis')}
            <SortHead
              label="Cost Basis"
              active={sortBy === 'costBasis'}
              dir={sortDir}
              onclick={() => sort.toggle('costBasis')}
              class="text-right"
            />
          {/if}
          {#if isVisible('gainLoss')}
            <SortHead
              label="Gain / Loss"
              active={sortBy === 'gainLoss'}
              dir={sortDir}
              onclick={() => sort.toggle('gainLoss')}
              class="text-right"
            />
          {/if}
          {#if isVisible('alloc')}
            <SortHead
              label="Alloc"
              active={sortBy === 'alloc'}
              dir={sortDir}
              onclick={() => sort.toggle('alloc')}
              class="text-right"
            />
          {/if}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#if filteredHoldings.length === 0}
          <Table.Row>
            <Table.Cell class="h-24 text-center text-muted-foreground" colspan={skeletonCols}>
              No holdings match “{searchInput.trim()}”.
            </Table.Cell>
          </Table.Row>
        {:else}
          {#each sortedHoldings as h (h.commodityGuid)}
            {@const isClosed = h.shares === 0 && h.accounts.length > 0}
            {@const isWatchOnly = h.shares === 0 && h.accounts.length === 0}
            <NavRow
              onNavigate={() => openHolding(h.commodityGuid)}
              class={isClosed || isWatchOnly ? 'text-muted-foreground' : ''}
            >
              <Table.Cell class="py-2.5">
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
                <Table.Cell class="py-2.5 text-right tabular-nums"
                  >{formatShares(h.shares)}</Table.Cell
                >
              {/if}
              {#if isVisible('avgCost')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {h.shares > 0 ? fmt(h.avgCostPerShare) : EMPTY_DASH}
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
                    {EMPTY_DASH}
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
                  {formatAllocPct(h.allocationPct)}
                </Table.Cell>
              {/if}
            </NavRow>
          {/each}
        {/if}
      </Table.Body>
    </TableShell>
  </div>
{/if}
