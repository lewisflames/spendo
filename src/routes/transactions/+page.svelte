<script lang="ts">
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { ArrowLeftRight } from '@lucide/svelte';
  import {
    fetchTransactions,
    fetchAllTransactions,
    fetchAccountNames
  } from '$lib/transactions.remote';
  import { dbSession } from '$lib/stores/db.svelte';
  import { formatNumber } from '$lib/format.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import EmptyState from '$lib/components/empty-state.svelte';
  import TransactionFilters from '$lib/components/transactions/transaction-filters.svelte';
  import TransactionTable from '$lib/components/transactions/transaction-table.svelte';
  import Pagination from '$lib/components/transactions/pagination.svelte';
  import type { TransactionsResult, AccountInfo } from '$lib/server/transactions';

  let pageSize = $derived(appSettings.value.pageSize);

  let page = $state(1);

  let searchInput = $state('');
  let search = $state('');
  let dateFrom = $state('');
  let dateTo = $state('');
  let accountName = $state('');

  $effect(() => {
    const val = searchInput;
    const timer = setTimeout(() => {
      search = val;
    }, 300);
    return () => clearTimeout(timer);
  });

  let filters = $derived({
    ...(search ? { search } : {}),
    ...(dateFrom ? { dateFrom } : {}),
    ...(dateTo ? { dateTo } : {}),
    ...(accountName ? { accountName } : {})
  });

  let token = $derived(dbSession.value?.token ?? '');
  let currency = $derived(dbSession.value?.summary.currency ?? 'EUR');

  let accounts = $state<AccountInfo[]>([]);

  $effect(() => {
    if (!token) {
      accounts = [];
      return;
    }
    let cancelled = false;
    fetchAccountNames({ token }).then(
      (r) => {
        if (cancelled) return;
        accounts = r;
      },
      () => {}
    );
    return () => {
      cancelled = true;
    };
  });

  let loading = $state(false);
  let data = $state<TransactionsResult | null>(null);
  let fetchError = $state(false);

  let prevKey = '';

  $effect(() => {
    if (!token) {
      data = null;
      loading = false;
      fetchError = false;
      return;
    }

    const fk = JSON.stringify(filters);
    if (prevKey && prevKey !== fk) {
      prevKey = fk;
      if (page !== 1) {
        page = 1;
        return;
      }
    }
    prevKey = fk;

    let cancelled = false;

    loading = true;
    data = null;
    fetchError = false;

    fetchTransactions({ token, page, pageSize, filters }).then(
      (r) => {
        if (cancelled) return;
        data = r;
        loading = false;
      },
      () => {
        if (cancelled) return;
        fetchError = true;
        loading = false;
      }
    );

    return () => {
      cancelled = true;
    };
  });

  function escapeCsv(val: string): string {
    if (val.includes(',') || val.includes('"') || val.includes('\n')) {
      return `"${val.replace(/"/g, '""').replace(/\n/g, ' ')}"`;
    }
    return val;
  }

  async function downloadCSV() {
    const transactions = await fetchAllTransactions({ token, filters });

    const delimiter = ',';
    const header = ['Date', 'Description', 'Account', 'Type', 'Amount'].join(delimiter);
    const rows = transactions.flatMap((txn) =>
      txn.splits.map((s) =>
        [
          txn.postDate,
          escapeCsv(txn.description),
          escapeCsv(s.accountName),
          s.accountType,
          s.amount
        ].join(delimiter)
      )
    );
    const csv = [header, ...rows].join('\n');

    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `transactions-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<svelte:head>
  <title>Transactions - Spendo</title>
</svelte:head>

{#if !token}
  <EmptyState title="No Data" description="Upload a GnuCash file to see your transactions.">
    {#snippet icon()}
      <ArrowLeftRight aria-hidden="true" class="size-12 text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <div class="flex flex-col gap-4">
    <div class="flex items-center justify-between">
      <div class="flex flex-col gap-1">
        <h2 class="text-3xl font-bold tracking-tight">Transactions</h2>
        <p class="text-sm text-muted-foreground">
          {data
            ? `${formatNumber(data.total)} transaction${data.total === 1 ? '' : 's'}`
            : 'Loading...'}
        </p>
      </div>
    </div>

    <TransactionFilters
      bind:search={searchInput}
      bind:dateFrom
      bind:dateTo
      bind:accountName
      {accounts}
      showExport={!!data}
      onExport={downloadCSV}
    />

    {#if loading}
      <div class="overflow-x-auto rounded-lg border">
        <table class="w-full min-w-150">
          <thead>
            <tr class="bg-muted/50 text-left text-sm text-muted-foreground">
              <th class="px-4 py-3 font-medium">Date</th>
              <th class="px-4 py-3 font-medium">Description</th>
              <th class="px-4 py-3 font-medium">Debit</th>
              <th class="px-4 py-3 font-medium">Credit</th>
              <th class="px-4 py-3 text-right font-medium">Amount</th>
            </tr>
          </thead>
          <tbody>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
            <tr><td class="px-4 py-3" colspan="5"><Skeleton class="h-5 w-full" /></td></tr>
          </tbody>
        </table>
      </div>
    {:else if fetchError}
      <div
        class="rounded-lg border border-destructive/50 bg-destructive/10 px-4 py-8 text-center text-sm text-destructive"
      >
        Failed to load transactions. Your session may have expired.
      </div>
    {:else if data}
      <TransactionTable transactions={data.transactions} {currency} />
      <Pagination bind:page totalPages={data.totalPages} />
    {/if}
  </div>
{/if}
