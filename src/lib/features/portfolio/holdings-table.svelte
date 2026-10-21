<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { resolve } from '$app/paths';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import ToolbarSkeleton from '$lib/components/shared/toolbar-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import FilteredEmptyState from '$lib/components/shared/filtered-empty-state.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { SearchInput } from '$lib/components/shared/search-input';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Plus, TrendingUp } from '@lucide/svelte';
  import type { PortfolioHolding } from '$lib/tauri';
  import { STALE_PRICE_DAYS, priceStaleDays } from './price-utils';
  import { openHolding } from '$lib/utils/navigation';
  import { openAccountDialog } from '$lib/features/accounts/account-dialog-state.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import { useUrlParams, debouncedSearchParam } from '$lib/hooks/use-url-params.svelte';
  import { computeRebalance, TRADE_DUST } from './rebalance';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import {
    formatCurrency,
    formatDate,
    signColor,
    fmtPct,
    formatSignedCurrency,
    formatShares,
    formatAllocPct
  } from '$lib/utils/format.svelte';

  type SortKey =
    | 'name'
    | 'shares'
    | 'avgCost'
    | 'price'
    | 'marketValue'
    | 'costBasis'
    | 'gainLoss'
    | 'xirr'
    | 'alloc'
    | 'target'
    | 'buy';

  type ColumnId = Exclude<SortKey, 'name'>;

  const COLUMNS: { id: ColumnId; label: string }[] = [
    { id: 'shares', label: 'Shares' },
    { id: 'avgCost', label: 'Avg cost' },
    { id: 'price', label: 'Price' },
    { id: 'marketValue', label: 'Market value' },
    { id: 'costBasis', label: 'Cost basis' },
    { id: 'gainLoss', label: 'Gain / loss' },
    { id: 'xirr', label: 'XIRR' },
    { id: 'alloc', label: 'Alloc' },
    { id: 'target', label: 'Target' },
    { id: 'buy', label: 'Buy' }
  ];

  const DEFAULT_COLUMNS: ColumnId[] = [
    'shares',
    'price',
    'marketValue',
    'gainLoss',
    'alloc',
    'target',
    'buy'
  ];

  let {
    holdings = [] as PortfolioHolding[],
    currency,
    targets,
    loading = false
  }: {
    holdings: PortfolioHolding[];
    currency: string;
    targets: ReadonlyMap<string, number>;
    loading?: boolean;
  } = $props();

  let sort = useTableSort<SortKey>({ sortBy: 'marketValue', sortDir: 'desc' }, [
    'shares',
    'avgCost',
    'price',
    'marketValue',
    'costBasis',
    'gainLoss',
    'xirr',
    'alloc',
    'target',
    'buy'
  ]);
  let sortBy = $derived(sort.sortBy);
  let sortDir = $derived(sort.sortDir);

  const cols = useColumnVisibility<ColumnId>(DEFAULT_COLUMNS, 'cols:holdings');
  let visibleColumns = $derived(cols.visibleColumns);

  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);

  useUrlParams([
    debouncedSearchParam(
      'q',
      () => search.value || null,
      (v) => {
        searchInput = v;
        search.set(v);
      }
    )
  ]);

  let filteredHoldings = $derived.by(() => {
    const q = search.value.trim().toLowerCase();
    if (!q) return holdings;
    return holdings.filter(
      (h) => (h.fullName ?? '').toLowerCase().includes(q) || h.mnemonic.toLowerCase().includes(q)
    );
  });

  let result = $derived(computeRebalance(holdings, targets));
  let rebalanceByGuid = $derived(new Map(result.rows.map((r) => [r.commodityGuid, r])));

  let skeletonCols = $derived(visibleColumns.length + 1);

  // Hiding the active sort column would leave an invisible sort in effect;
  // fall back to the always-visible name column.
  function handleHideColumn(id: ColumnId) {
    if (sortBy === id) sort.toggle('name');
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
        case 'xirr':
          cmp = nullCmp(a.xirr, b.xirr) || (a.xirr ?? 0) - (b.xirr ?? 0);
          break;
        case 'alloc':
          cmp = a.allocationPct - b.allocationPct;
          break;
        case 'target': {
          const ra = rebalanceByGuid.get(a.commodityGuid)?.targetPct ?? null;
          const rb = rebalanceByGuid.get(b.commodityGuid)?.targetPct ?? null;
          cmp = nullCmp(ra, rb) || (ra ?? 0) - (rb ?? 0);
          break;
        }
        case 'buy': {
          const ra = rebalanceByGuid.get(a.commodityGuid)?.buyAmount ?? null;
          const rb = rebalanceByGuid.get(b.commodityGuid)?.buyAmount ?? null;
          cmp = nullCmp(ra, rb) || (ra ?? 0) - (rb ?? 0);
          break;
        }
      }
      return cmp * dir;
    });
  });

  let totals = $derived.by(() => {
    let marketValue = 0;
    let costBasis = 0;
    let gainLoss = 0;
    let alloc = 0;
    let targetPct = 0;
    let buyAmount = 0;
    for (const h of filteredHoldings) {
      marketValue += h.marketValue;
      costBasis += h.costBasis;
      gainLoss += h.gainLoss;
      alloc += h.allocationPct;
      const row = rebalanceByGuid.get(h.commodityGuid);
      if (row) {
        targetPct += row.targetPct;
        buyAmount += row.buyAmount;
      }
    }
    return {
      marketValue,
      costBasis,
      gainLoss,
      alloc,
      targetPct,
      buyAmount
    };
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
  <EmptyState
    card
    title="No investments yet"
    description="Once your book holds investments, say after a GnuCash import, they’ll land here."
  >
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
    {#snippet action()}
      <Button variant="outline" onclick={openAccountDialog}>
        <Plus aria-hidden="true" class="size-4" />
        New account
      </Button>
    {/snippet}
  </EmptyState>
{:else}
  <!-- One wrapper so the page-level flex gap doesn’t open up between toolbar and table. -->
  <div class="flex flex-col gap-4">
    <FilterBar>
      {#snippet extra()}
        <ColumnsMenu columns={COLUMNS} {cols} onHide={handleHideColumn} />
      {/snippet}
      <SearchInput
        id="holdings-search"
        label="Search holdings"
        placeholder="Search by name or ticker…"
        bind:value={searchInput}
        class="grow basis-48"
      />
      {#if search.value}
        <FilterClearButton
          onclick={() => {
            searchInput = '';
            search.set('');
          }}
        />
      {/if}
    </FilterBar>
    {#if filteredHoldings.length === 0}
      <FilteredEmptyState
        title="No matching holdings"
        onClear={() => {
          searchInput = '';
          search.set('');
        }}
      />
    {:else}
      <TableShell>
        <Table.Header>
          <Table.Row class="bg-muted/75">
            <SortHead
              label="Holding"
              active={sortBy === 'name'}
              dir={sortDir}
              onclick={() => sort.toggle('name')}
            />
            {#each COLUMNS as col (col.id)}
              {#if cols.isVisible(col.id)}
                <SortHead
                  label={col.label}
                  active={sortBy === col.id}
                  dir={sortDir}
                  onclick={() => sort.toggle(col.id)}
                  align="right"
                />
              {/if}
            {/each}
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each sortedHoldings as h (h.commodityGuid)}
            {@const isClosed = h.shares === 0 && h.accounts.length > 0}
            {@const isWatchOnly = h.shares === 0 && h.accounts.length === 0}
            {@const rebalance = rebalanceByGuid.get(h.commodityGuid)}
            <NavRow
              onNavigate={() => openHolding(h.commodityGuid)}
              class={isClosed || isWatchOnly
                ? 'cursor-pointer text-muted-foreground hover:bg-muted/50'
                : 'cursor-pointer hover:bg-muted/50'}
            >
              <Table.Cell class="py-2.5">
                <!-- Real link for keyboard/AT users; the row click stays as a
                 convenience target for pointer input. -->
                <a
                  href={resolve('/portfolio/[holding]', { holding: h.commodityGuid })}
                  class="flex flex-col gap-0.5 focus-visible:outline-2 focus-visible:outline-ring"
                >
                  <span class="flex items-center gap-2 font-medium">
                    <span
                      class="truncate"
                      {@attach tooltip(h.fullName ?? h.mnemonic, { truncated: true })}
                      >{h.fullName ?? h.mnemonic}</span
                    >
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
              {#if cols.isVisible('shares')}
                <Table.Cell class="py-2.5 text-right tabular-nums"
                  >{formatShares(h.shares)}</Table.Cell
                >
              {/if}
              {#if cols.isVisible('avgCost')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if h.shares > 0}{fmt(h.avgCostPerShare)}{:else}<EmptyValue />{/if}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('price')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if h.price !== null}
                    {@const stale =
                      h.priceDate !== null && priceStaleDays(h.priceDate) > STALE_PRICE_DAYS}
                    <span
                      class={stale ? 'text-warning' : ''}
                      {@attach tooltip(h.priceDate === null ? undefined : formatDate(h.priceDate))}
                    >
                      {fmt(h.price)}
                    </span>
                  {:else}
                    <EmptyValue />
                  {/if}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('marketValue')}
                <Table.Cell class="py-2.5 text-right font-medium tabular-nums">
                  {fmt(h.marketValue)}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('costBasis')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {fmt(h.costBasis)}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('gainLoss')}
                <Table.Cell class="py-2.5 text-right tabular-nums {signColor(h.gainLoss)}">
                  {formatSignedCurrency(h.gainLoss, currency)}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('xirr')}
                <Table.Cell
                  class="py-2.5 text-right tabular-nums {h.xirr !== null
                    ? signColor(h.xirr)
                    : 'text-muted-foreground'}"
                >
                  {fmtPct(h.xirr)}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('alloc')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {formatAllocPct(h.allocationPct)}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('target')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if rebalance && rebalance.targetPct > 0}
                    {formatAllocPct(rebalance.targetPct)}
                  {:else}
                    <EmptyValue />
                  {/if}
                </Table.Cell>
              {/if}
              {#if cols.isVisible('buy')}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if rebalance && rebalance.buyAmount >= TRADE_DUST}
                    <span class="font-medium">{fmt(rebalance.buyAmount)}</span>
                    {#if rebalance.buyShares !== null}
                      <span class="block text-xs text-muted-foreground"
                        >≈ {formatShares(rebalance.buyShares)} shares</span
                      >
                    {/if}
                  {:else}
                    <EmptyValue />
                  {/if}
                </Table.Cell>
              {/if}
            </NavRow>
          {/each}
        </Table.Body>
        <Table.Footer>
          <Table.Row class="border-t bg-muted/50 font-semibold">
            <Table.Cell class="py-2.5 text-xs font-semibold tracking-wider uppercase"
              >Total</Table.Cell
            >
            {#each COLUMNS as col (col.id)}
              {#if cols.isVisible(col.id)}
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if col.id === 'marketValue'}
                    {fmt(totals.marketValue)}
                  {:else if col.id === 'costBasis'}
                    {fmt(totals.costBasis)}
                  {:else if col.id === 'gainLoss'}
                    <span class={signColor(totals.gainLoss)}>
                      {formatSignedCurrency(totals.gainLoss, currency)}
                    </span>
                  {:else if col.id === 'xirr'}
                    <!-- Rates don’t sum: the portfolio XIRR lives on the KPI card. -->
                    <EmptyValue />
                  {:else if col.id === 'alloc'}
                    {formatAllocPct(totals.alloc)}
                  {:else if col.id === 'target'}
                    {#if totals.targetPct > 0}
                      {formatAllocPct(totals.targetPct)}
                    {:else}
                      <EmptyValue />
                    {/if}
                  {:else if col.id === 'buy'}
                    {#if totals.targetPct > 0}
                      {fmt(totals.buyAmount)}
                    {:else}
                      <EmptyValue />
                    {/if}
                  {/if}
                </Table.Cell>
              {/if}
            {/each}
          </Table.Row>
        </Table.Footer>
      </TableShell>
    {/if}
  </div>
{/if}
