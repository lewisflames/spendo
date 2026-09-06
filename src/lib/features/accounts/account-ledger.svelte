<script lang="ts">
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Button } from '$lib/components/ui/button';
  import { Columns3, Receipt } from '@lucide/svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { appSettings } from '$lib/stores/settings.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TransactionFilters from '$lib/features/transactions/transaction-filters.svelte';
  import TransactionTable from '$lib/features/transactions/transaction-table.svelte';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import type { AccountDetailResult } from '$lib/tauri';

  type ColumnId = 'transfer' | 'status' | 'amount' | 'runningBalance';

  const COLUMNS: { id: ColumnId; label: string }[] = [
    { id: 'transfer', label: 'Transfer' },
    { id: 'status', label: 'Status' },
    { id: 'amount', label: 'Amount' },
    { id: 'runningBalance', label: 'Balance' }
  ];

  const DEFAULT_COLUMNS: ColumnId[] = ['transfer', 'amount'];

  type SortKey = 'date' | 'description' | 'amount';

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
    sortBy: SortKey;
    sortDir: 'asc' | 'desc';
    onSort: (key: SortKey) => void;
    currency: string;
    page: number;
    onPageSizeChange: (size: number) => void;
    onRetry: () => void;
  } = $props();

  const cols = useColumnVisibility<ColumnId>(DEFAULT_COLUMNS);
  let visibleColumns = $derived(cols.visibleColumns);
  let skeletonCols = $derived(visibleColumns.length + 2);
</script>

<!-- Filters -->
<TransactionFilters bind:search={searchInput} bind:dateFrom bind:dateTo>
  {#snippet extra()}
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="outline">
            <Columns3 aria-hidden="true" class="text-muted-foreground" />
            Columns
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end">
        {#each COLUMNS as col (col.id)}
          <DropdownMenu.CheckboxItem
            checked={cols.isVisible(col.id)}
            onCheckedChange={(checked) => cols.setVisible(col.id, checked)}
            closeOnSelect={false}
          >
            {col.label}
          </DropdownMenu.CheckboxItem>
        {/each}
        <DropdownMenu.Separator />
        <DropdownMenu.Item onclick={() => cols.resetColumns()}>Reset to default</DropdownMenu.Item>
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  {/snippet}
</TransactionFilters>

{#if loading && !data}
  <TableSkeleton cols={skeletonCols} rows={5} />
{:else if error}
  <ErrorCard message={error} {onRetry} />
{:else if data}
  {@const ledger = data}

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
{/if}
