<script lang="ts">
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import {
    fetchAccounts,
    fetchTags,
    fetchTransactions,
    type BulkAction,
    type BulkEditResult,
    type BulkTarget,
    type TransactionRow
  } from '$lib/tauri';
  import {
    activeBookPath,
    dbBootstrapping,
    dbSession,
    refreshSession
  } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import TransactionFilters from '$lib/features/transactions/transaction-filters.svelte';
  import TransactionTable, {
    TRANSACTION_COLUMNS,
    TRANSACTION_DEFAULT_COLUMNS
  } from '$lib/features/transactions/transaction-table.svelte';
  import TransactionHeatmap from '$lib/features/transactions/transaction-heatmap.svelte';
  import BulkEditBar from '$lib/features/transactions/bulk-edit-bar.svelte';
  import BulkEditDialog from '$lib/features/transactions/bulk-edit-dialog.svelte';
  import {
    bulkResultMessage,
    pageSelectionState,
    rangeRows,
    selectionAccounts,
    selectionTags
  } from '$lib/features/transactions/bulk-edit';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import { Plus } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import EmptyBookPrompt from '$lib/components/shared/empty-book-prompt.svelte';
  import FilteredEmptyState from '$lib/components/shared/filtered-empty-state.svelte';
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
  import { ArrowLeftRight } from '@lucide/svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import type { TransactionColumnId } from '$lib/features/transactions/transaction-table.svelte';

  const meta = PAGE_META.transactions;

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
  let tagGuid = $state('');

  let search = useDebouncedSearch(() => searchInput);

  /** Mirrors TransactionFilters' internal clear so both buttons stay in sync. */
  function clearFilters() {
    searchInput = '';
    search.set('');
    dateFrom = '';
    dateTo = '';
    tagGuid = '';
  }

  const cols = useColumnVisibility<TransactionColumnId>(
    TRANSACTION_DEFAULT_COLUMNS,
    'cols:transactions'
  );
  let visibleColumns = $derived(cols.visibleColumns);

  // Don’t leave an invisible sort in effect: prefer the still-visible date,
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
    ),
    // Tag pages and badges link here with `?tag=<guid>`.
    {
      key: 'tag',
      get: () => tagGuid || null,
      apply: (v) => (tagGuid = v ?? ''),
      replace: true
    }
  ]);

  let filters = $derived({
    ...(search.value ? { search: search.value } : {}),
    ...(dateFrom ? { dateFrom } : {}),
    ...(dateTo ? { dateTo } : {}),
    ...(tagGuid ? { tagGuid } : {})
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
    label: 'We couldn’t load your transactions'
  });

  // Tags for the filter and the bulk dialog; reloaded after a bulk edit since
  // tagging can create one.
  const tagsRes = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchTags,
    label: 'We couldn’t load your tags'
  });
  let allTags = $derived(tagsRes.data ?? []);

  // ── Bulk edit ──
  // Selected rows keep their loaded data so the dialog can offer the accounts
  // they post to, across pages. "All matching" reaches rows never loaded, so
  // it targets the filters instead.
  const selected = new SvelteMap<string, TransactionRow>();
  let allMatching = $state(false);
  /** Last toggled row: the shift-click range anchor. Not reactive on purpose. */
  let anchor: string | null = null;
  let bulkKind = $state<BulkAction['kind']>('moveSplits');
  let bulkOpen = $state(false);
  let heatmapKey = $state(0);

  let rows = $derived(res.data?.transactions ?? []);
  let total = $derived(res.data?.total ?? 0);
  let hasSelection = $derived(allMatching || selected.size > 0);

  function clearSelection() {
    selected.clear();
    allMatching = false;
    anchor = null;
  }

  // A different query or book makes the selection meaningless.
  useResetPageOnChange(() => JSON.stringify(filters), clearSelection);
  useResetPageOnChange(() => activeBookPath.value, clearSelection);

  function toggleRow(txn: TransactionRow, next: boolean) {
    allMatching = false;
    if (next) selected.set(txn.guid, txn);
    else selected.delete(txn.guid);
    anchor = txn.guid;
  }

  function togglePage(next: boolean) {
    allMatching = false;
    for (const txn of rows) {
      if (next) selected.set(txn.guid, txn);
      else selected.delete(txn.guid);
    }
  }

  function selectRange(guid: string) {
    allMatching = false;
    for (const txn of rangeRows(rows, anchor, guid)) selected.set(txn.guid, txn);
    anchor = guid;
  }

  let bulkTarget = $derived<BulkTarget>(
    allMatching
      ? {
          kind: 'matching',
          filters: Object.keys(filters).length > 0 ? filters : null,
          expectedCount: total
        }
      : { kind: 'guids', guids: [...selected.keys()] }
  );

  let sharedDescription = $derived.by(() => {
    if (allMatching) return '';
    const descriptions = new Set([...selected.values()].map((t) => t.description ?? ''));
    return descriptions.size === 1 ? [...descriptions][0] : '';
  });

  const accountsRes = useAsyncData({
    enabled: () => !!dbSession.value && hasSelection,
    fetch: fetchAccounts,
    label: 'We couldn’t load your accounts'
  });
  let pickerAccounts = $derived(flattenAccountsForPicker(accountsRes.data?.trees));

  function openBulk(kind: BulkAction['kind']) {
    bulkKind = kind;
    bulkOpen = true;
  }

  function handleBulkDone(kind: BulkAction['kind'], result: BulkEditResult) {
    const message = bulkResultMessage(kind, result);
    if (result.changed > 0) toast.success(message);
    else toast.info(message);
    clearSelection();
    heatmapKey += 1;
    res.reload();
    tagsRes.reload();
    refreshSession();
  }
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader
      title={meta.title}
      description={res.data
        ? `${meta.description} · ${plural(res.data.total, 'transaction')}`
        : meta.description}
    />

    <TransactionHeatmap {filters} refreshKey={heatmapKey} />

    <TransactionFilters
      bind:search={searchInput}
      bind:dateFrom
      bind:dateTo
      bind:tagGuid
      tags={allTags}
    >
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
        {@const isFiltered = !!(search.value || dateFrom || dateTo || tagGuid)}
        {#if isFiltered}
          <FilteredEmptyState title="No matching transactions" onClear={clearFilters} />
        {:else if dbSession.value?.summary.totalAccounts === 0}
          <EmptyBookPrompt />
        {:else}
          <EmptyState
            card
            title="No transactions yet"
            description="Record your first one. It takes a few seconds."
          >
            {#snippet icon()}
              <ArrowLeftRight aria-hidden="true" class="text-muted-foreground" />
            {/snippet}
            {#snippet action()}
              <Button href={resolve('/transactions/new')}>
                <Plus aria-hidden="true" class="size-4" />
                New transaction
              </Button>
            {/snippet}
          </EmptyState>
        {/if}
      {:else}
        <!-- Keep the previous table visible (dimmed) during refetches so the
             page doesn’t jump and pagination stays in place. -->
        <DimWrapper active={res.loading} class="flex flex-col gap-4">
          <TransactionTable
            transactions={res.data.transactions}
            {currency}
            {sortBy}
            {sortDir}
            onSort={sort.toggle}
            {visibleColumns}
            {selected}
            onToggle={toggleRow}
            onToggleAll={togglePage}
            onRangeSelect={selectRange}
          />
          <PaginationNav
            bind:page
            totalPages={res.data.totalPages}
            totalItems={res.data.total}
            pageSize={appSettings.value.pageSize}
            onPageSizeChange={(size) => applyPageSize(size, (v) => (page = v))}
          />
        </DimWrapper>
        <!-- Below the table and sticky to the viewport bottom: showing it never
             pushes the rows the user is clicking. -->
        {#if hasSelection}
          <BulkEditBar
            count={selected.size}
            {total}
            pageFullySelected={pageSelectionState(selected, rows) === 'all'}
            {allMatching}
            onSelectAllMatching={() => (allMatching = true)}
            onClear={clearSelection}
            onAction={openBulk}
          />
        {/if}
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>

<BulkEditDialog
  bind:open={bulkOpen}
  kind={bulkKind}
  target={bulkTarget}
  count={allMatching ? total : selected.size}
  accounts={pickerAccounts}
  selectionAccountGuids={selectionAccounts(selected.values()).map((a) => a.guid)}
  anySource={allMatching}
  defaultDescription={sharedDescription}
  selectionTagList={selectionTags(selected.values())}
  {allTags}
  onDone={handleBulkDone}
/>
