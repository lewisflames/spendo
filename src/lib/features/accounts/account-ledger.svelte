<script lang="ts">
  import { Receipt } from '@lucide/svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { appSettings } from '$lib/stores/settings.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TransactionFilters from '$lib/features/transactions/transaction-filters.svelte';
  import TransactionTable from '$lib/features/transactions/transaction-table.svelte';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import type { AccountDetailResult, TxSortKey } from '$lib/tauri';

  type ColumnId = 'transfer' | 'status' | 'amount' | 'runningBalance';

  const COLUMNS: { id: ColumnId; label: string }[] = [
    { id: 'transfer', label: 'Transfer' },
    { id: 'status', label: 'Status' },
    { id: 'amount', label: 'Amount' },
    { id: 'runningBalance', label: 'Balance' }
  ];

  const DEFAULT_COLUMNS: ColumnId[] = ['transfer', 'amount'];

  let {
    data,
    loading,
    error,
    searchInput = $bindable(),
    dateFrom = $bindable(),
    dateTo = $bindable(),
    searchText,
    sortBy,
    sortDir,
    onSort,
    currency,
    page = $bindable(),
    onPageSizeChange,
    onRetry
  }: {
    data: AccountDetailResult | null;
    loading: boolean;
    error: string | null;
    searchInput: string;
    dateFrom: string;
    dateTo: string;
    /** Committed (debounced) search text for the empty-state message. */
    searchText: string;
    sortBy: TxSortKey;
    sortDir: 'asc' | 'desc';
    onSort: (key: TxSortKey) => void;
    currency: string;
    page: number;
    onPageSizeChange: (size: number) => void;
    onRetry: () => void;
  } = $props();

  const cols = useColumnVisibility<ColumnId>(DEFAULT_COLUMNS, 'cols:ledger');
  let visibleColumns = $derived(cols.visibleColumns);
  let skeletonCols = $derived(visibleColumns.length + 2);

  function handleHideColumn(id: ColumnId) {
    // Amount is the only hideable column that is also sortable — fall back to
    // date so an invisible sort can't stay in effect.
    if (id === 'amount' && sortBy === 'amount') onSort('date');
  }
</script>

<!-- Filters -->
<TransactionFilters bind:search={searchInput} bind:dateFrom bind:dateTo>
  {#snippet extra()}
    <ColumnsMenu columns={COLUMNS} {cols} onHide={handleHideColumn} />
  {/snippet}
</TransactionFilters>

{#if loading && !data}
  <TableSkeleton cols={skeletonCols} rows={5} />
{:else if data}
  {@const ledger = data}
  {#if error}
    <ErrorCard message={error} {onRetry} />
  {/if}

  {#if ledger.transactions.length === 0}
    <EmptyState
      card
      title="No transactions"
      description="No transactions found{searchText || dateFrom || dateTo
        ? ' matching the current filters'
        : ''}."
    >
      {#snippet icon()}
        <Receipt aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
    </EmptyState>
  {:else}
    <!-- Keep the previous table visible (dimmed) during refetches so the
         page doesn't jump and pagination stays in place. -->
    <DimWrapper active={loading} class="flex flex-col gap-4">
      <TransactionTable
        transactions={ledger.transactions}
        {currency}
        {sortBy}
        {sortDir}
        {onSort}
        variant="account"
        {visibleColumns}
      />

      <PaginationNav
        bind:page
        totalPages={ledger.totalPages}
        totalItems={ledger.total}
        pageSize={appSettings.value.pageSize}
        {onPageSizeChange}
      />
    </DimWrapper>
  {/if}
{:else if error}
  <ErrorCard message={error} {onRetry} />
{/if}
