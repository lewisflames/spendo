<script lang="ts" module>
  export type PriceColumnId = 'price' | 'change';

  export const PRICE_COLUMNS: { id: PriceColumnId; label: string }[] = [
    { id: 'price', label: 'Price' },
    { id: 'change', label: 'Change' }
  ];

  export const PRICE_DEFAULT_COLUMNS: PriceColumnId[] = ['price', 'change'];
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import * as Table from '$lib/components/ui/table';
  import { TrendingUp } from '@lucide/svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import type { ColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import {
    signColor,
    formatCurrency,
    formatDate,
    fmtPct,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import type { PriceHistoryEntry } from '$lib/tauri';

  let {
    priceHistory,
    priceCurrency,
    cols
  }: {
    priceHistory: PriceHistoryEntry[];
    priceCurrency: string;
    cols: ColumnVisibility<PriceColumnId>;
  } = $props();

  function isVisible(id: PriceColumnId): boolean {
    return cols.isVisible(id);
  }

  interface PriceRow extends PriceHistoryEntry {
    /** Day-over-day change in percent (0–100 scale); null for the oldest entry. */
    changePct: number | null;
  }

  /** Chronological (asc) pass to compute change, then newest-first for display. */
  let rowsDesc = $derived.by((): PriceRow[] => {
    const asc = [...priceHistory].sort((a, b) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0));
    const withChange: PriceRow[] = asc.map((e, i) => {
      if (i === 0) return { ...e, changePct: null };
      const prev = asc[i - 1].price;
      if (prev === 0) return { ...e, changePct: null };
      return { ...e, changePct: ((e.price - prev) / Math.abs(prev)) * 100 };
    });
    return withChange.reverse();
  });

  let total = $derived(rowsDesc.length);
  let pageSize = $derived(appSettings.value.pageSize);
  let totalPages = $derived(Math.max(1, Math.ceil(total / pageSize)));
  let page = $state(1);

  // Clamp back into range when the history shrinks or the page size grows.
  $effect(() => {
    if (page > totalPages) page = totalPages;
  });
  $effect(() => {
    // New data (e.g. after a price refresh or holding switch) starts back on
    // page 1. `page` is read untracked so navigating pages doesn't re-trigger
    // this effect and snap back to 1.
    void priceHistory.length;
    untrack(() => {
      if (page !== 1) page = 1;
    });
  });

  let pageRows = $derived(rowsDesc.slice((page - 1) * pageSize, page * pageSize));
</script>

<!-- Price history -->
{#if total === 0}
  <EmptyState
    card
    title="No prices"
    description="No price history found — set the Yahoo symbol, then refresh prices."
  >
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <div class="flex flex-col gap-4">
    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="text-muted-foreground">Date</Table.Head>
          {#if isVisible('price')}
            <Table.Head class="text-right text-muted-foreground">Price</Table.Head>
          {/if}
          {#if isVisible('change')}
            <Table.Head class="text-right text-muted-foreground">Change</Table.Head>
          {/if}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each pageRows as row (row.date)}
          <Table.Row>
            <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {formatDate(row.date)}
            </Table.Cell>
            {#if isVisible('price')}
              <Table.Cell class="py-2.5 text-right font-medium tabular-nums">
                {formatCurrency(row.price, priceCurrency)}
              </Table.Cell>
            {/if}
            {#if isVisible('change')}
              <Table.Cell
                class="py-2.5 text-right tabular-nums {row.changePct === null
                  ? 'text-muted-foreground'
                  : signColor(row.changePct)}"
              >
                {row.changePct === null ? EMPTY_DASH : fmtPct(row.changePct)}
              </Table.Cell>
            {/if}
          </Table.Row>
        {/each}
      </Table.Body>
    </TableShell>
    <PaginationNav
      bind:page
      {totalPages}
      totalItems={total}
      {pageSize}
      onPageSizeChange={(size) => applyPageSize(size, (v) => (page = v))}
    />
  </div>
{/if}
