<script lang="ts">
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { fetchTransactions } from '$lib/tauri';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import TransactionFilters from '$lib/features/transactions/transaction-filters.svelte';
  import TransactionTable, {
    TRANSACTION_COLUMNS,
    TRANSACTION_DEFAULT_COLUMNS
  } from '$lib/features/transactions/transaction-table.svelte';
  import TransactionHeatmap from '$lib/features/transactions/transaction-heatmap.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Plus } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import {
    useUrlParams,
    pageParam,
    debouncedSearchParam,
    dateParam
  } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import { useResetPageOnChange } from '$lib/hooks/use-page-reset.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import { plural, sessionCurrency } from '$lib/utils/format.svelte';
  import { ArrowLeftRight, CalendarClock } from '@lucide/svelte';
  import type { TransactionColumnId } from '$lib/features/transactions/transaction-table.svelte';

  let pageSize = $derived(appSettings.value.pageSize);

  let page = $state(1);
  const sort = useTableSort<'date' | 'description' | 'amount'>(
    { sortBy: 'date', sortDir: 'desc' },
    ['date', 'amount']
  );
  let sortBy = $derived(sort.sortBy);
  let sortDir = $derived(sort.sortDir);

  let searchInput = $state('');
  let dateFrom = $state('');
  let dateTo = $state('');

  let search = useDebouncedSearch(() => searchInput);

  const cols = useColumnVisibility<TransactionColumnId>(
    TRANSACTION_DEFAULT_COLUMNS,
    'cols:transactions:v1'
  );
  let visibleColumns = $derived(cols.visibleColumns);

  // Don't leave an invisible sort in effect: prefer the still-visible date,
  // else the always-visible description.
  const SORT_FALLBACKS = ['date', 'description', 'amount'] as const;
  function handleHideColumn(id: TransactionColumnId) {
    if (id !== sortBy) return;
    const next = SORT_FALLBACKS.find(
      (key) => key !== id && (key === 'description' || cols.isVisible(key))
    );
    sort.toggle(next ?? 'description');
  }

  useUrlParams([
    pageParam(
      'page',
      () => page,
      (p) => (page = p)
    ),
    debouncedSearchParam(
      'q',
      () => search.value || null,
      (v) => {
        searchInput = v;
        search.set(v);
      }
    ),
    dateParam(
      'from',
      () => dateFrom || null,
      (v) => (dateFrom = v ?? '')
    ),
    dateParam(
      'to',
      () => dateTo || null,
      (v) => (dateTo = v ?? '')
    )
  ]);

  let filters = $derived({
    ...(search.value ? { search: search.value } : {}),
    ...(dateFrom ? { dateFrom } : {}),
    ...(dateTo ? { dateTo } : {})
  });

  let currency = $derived(sessionCurrency());

  // Reset to page 1 whenever the query shape changes. Runs before the fetch
  // effect in the same flush, so the fetch sees the reset page immediately.
  useResetPageOnChange(
    () => JSON.stringify({ filters, sortBy, sortDir }),
    () => {
      page = 1;
    }
  );

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: async () => {
      let r = await fetchTransactions(page, pageSize, filters, sortBy, sortDir);
      // The backend serves out-of-range pages as empty rows instead of
      // clamping; snap back to the last valid page (possible after
      // shrinking pageSize or narrowing filters).
      if (page > r.totalPages) {
        page = r.totalPages;
        r = await fetchTransactions(page, pageSize, filters, sortBy, sortDir);
      }
      return r;
    },
    label: 'Failed to load transactions'
  });
</script>

<svelte:head>
  <title>Transactions — Spendo</title>
  <meta name="description" content="Transaction ledger with filters." />
</svelte:head>

<BookGate title="Transactions" description="Transaction ledger with filters">
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Transactions"
      description={res.data ? plural(res.data.total, 'transaction') : ''}
    >
      {#if dbSession.value}
        <Button variant="outline" href={resolve('/scheduled')}>
          <CalendarClock aria-hidden="true" class="size-4" />
          Recurring
        </Button>
        <Button href={resolve('/transactions/new')}>
          <Plus aria-hidden="true" class="size-4" />
          New transaction
        </Button>
      {/if}
    </PageHeader>

    <TransactionHeatmap {filters} />

    <TransactionFilters bind:search={searchInput} bind:dateFrom bind:dateTo>
      {#snippet extra()}
        <ColumnsMenu columns={TRANSACTION_COLUMNS} {cols} onHide={handleHideColumn} />
      {/snippet}
    </TransactionFilters>

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <!-- Pagination placeholder holds its height so nothing jumps on first load. -->
      <PageSkeleton variant="transactions" />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if res.data.transactions.length === 0}
        <EmptyState
          card
          title="No transactions"
          description="No transactions found{search.value || dateFrom || dateTo
            ? ' matching the current filters'
            : ''}."
        >
          {#snippet icon()}
            <ArrowLeftRight aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
        </EmptyState>
      {:else}
        <!-- Keep the previous table visible (dimmed) during refetches so the
             page doesn't jump and pagination stays in place. -->
        <DimWrapper active={res.loading} class="flex flex-col gap-4">
          <TransactionTable
            transactions={res.data.transactions}
            {currency}
            {sortBy}
            {sortDir}
            onSort={sort.toggle}
            {visibleColumns}
          />
          <PaginationNav
            bind:page
            totalPages={res.data.totalPages}
            totalItems={res.data.total}
            pageSize={appSettings.value.pageSize}
            onPageSizeChange={(size) => applyPageSize(size, (v) => (page = v))}
          />
        </DimWrapper>
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
