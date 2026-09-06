<script lang="ts">
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { Button } from '$lib/components/ui/button';
  import { fetchTransactions, fetchAccountNames } from '$lib/tauri';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import TransactionFilters from '$lib/features/transactions/transaction-filters.svelte';
  import TransactionTable from '$lib/features/transactions/transaction-table.svelte';
  import TransactionHeatmap from '$lib/features/transactions/transaction-heatmap.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { downloadCsv } from '$lib/utils/csv';
  import { fetchAllRows } from '$lib/utils/export';
  import type { TransactionRow } from '$lib/tauri';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import {
    useUrlParams,
    pageParam,
    debouncedSearchParam,
    dateParam
  } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import { plural, sessionCurrency } from '$lib/utils/format.svelte';
  import { ArrowLeftRight, Download, LoaderCircle } from '@lucide/svelte';
  import type { AccountInfo } from '$lib/tauri';

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
  let accountName = $state('');

  let search = useDebouncedSearch(() => searchInput);

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
    {
      key: 'account',
      get: () => accountName || null,
      apply: (v) => (accountName = v ?? ''),
      replace: true
    }
  ]);

  let filters = $derived({
    ...(search.value ? { search: search.value } : {}),
    ...(dateFrom ? { dateFrom } : {}),
    ...(dateTo ? { dateTo } : {}),
    ...(accountName ? { accountName } : {})
  });

  let currency = $derived(sessionCurrency());

  let accounts = $state<AccountInfo[]>([]);

  useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchAccountNames,
    label: 'Failed to load accounts',
    onError: (message) => {
      accounts = [];
      toast.error(message);
    },
    onSuccess: (r) => {
      accounts = r;
    }
  });

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

  let loading = $derived(res.loading);
  let data = $derived(res.data);
  let error = $derived(res.error);

  let prevFilterKey = '';

  // Reset to page 1 whenever the query shape changes. Declared before the
  // hook's effect so it runs first in the same flush: the fetch sees the
  // reset page immediately instead of firing a doomed request.
  $effect(() => {
    const fk = JSON.stringify({ filters, sortBy, sortDir });
    if (prevFilterKey && prevFilterKey !== fk && page !== 1) {
      page = 1;
    }
    prevFilterKey = fk;
  });

  let exporting = $state(false);

  function handlePageSizeChange(size: number) {
    saveSettings({ pageSize: size });
    page = 1;
  }

  async function handleExportCsv() {
    if (!data || exporting) return;
    exporting = true;
    try {
      const all = await fetchAllRows<TransactionRow>(async (pageIdx) => {
        const r = await fetchTransactions(pageIdx, 500, filters, sortBy, sortDir);
        return { rows: r.transactions, total: r.total };
      });
      const headers = ['Date', 'Description', 'Number', 'Notes', 'Splits', 'Total'];
      const rows = all.map((t) => [
        t.postDate,
        t.description ?? '',
        t.num ?? '',
        t.notes ?? '',
        t.splits.map((s) => `${s.accountName} (${s.accountType}): ${s.amount}`).join(' | '),
        String(t.totalAmount)
      ]);
      downloadCsv('transactions.csv', headers, rows);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to export transactions'));
    } finally {
      exporting = false;
    }
  }
</script>

<svelte:head>
  <title>Transactions — Spendo</title>
  <meta name="description" content="Transaction ledger with filters." />
</svelte:head>

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader title="Transactions" description={data ? plural(data.total, 'transaction') : ''}>
      <Button
        variant="outline"
        onclick={handleExportCsv}
        disabled={!data || data.transactions.length === 0 || exporting}
      >
        {#if exporting}
          <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
        {:else}
          <Download aria-hidden="true" class="size-3" />
        {/if}
        Export
      </Button>
    </PageHeader>

    <TransactionHeatmap {filters} />

    <TransactionFilters
      bind:search={searchInput}
      bind:dateFrom
      bind:dateTo
      bind:accountName
      {accounts}
    />

    {#if loading && !data}
      <!-- Same wrapper as the loaded DimWrapper content so the pagination
           placeholder holds its height and nothing jumps on first load. -->
      <div class="flex flex-col gap-4">
        <TableSkeleton cols={3} />
        <div class="flex items-center justify-between">
          <Skeleton class="h-5 w-32" />
          <div class="flex items-center gap-2">
            <Skeleton class="h-8 w-24" />
            <Skeleton class="h-8 w-16" />
          </div>
        </div>
      </div>
    {:else if error}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {:else if data}
      {#if data.transactions.length === 0}
        <EmptyState
          card
          title="No transactions"
          description="No transactions found{search.value || dateFrom || dateTo || accountName
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
        <DimWrapper active={loading} class="flex flex-col gap-4">
          <TransactionTable
            transactions={data.transactions}
            {currency}
            {sortBy}
            {sortDir}
            onSort={sort.toggle}
          />
          <PaginationNav
            bind:page
            totalPages={data.totalPages}
            totalItems={data.total}
            pageSize={appSettings.value.pageSize}
            onPageSizeChange={handlePageSizeChange}
          />
        </DimWrapper>
      {/if}
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Transactions" description="Transaction ledger with filters" />
    <ImportPrompt />
  </div>
{/if}
