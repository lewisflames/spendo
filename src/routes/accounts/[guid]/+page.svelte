<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { goto } from '$app/navigation';
  import * as Table from '$lib/components/ui/table';
  import PaginationNav from '$lib/components/shared/pagination-nav';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import {
    TrendingUp,
    TrendingDown,
    Wallet,
    X,
    Receipt,
    Download,
    LoaderCircle
  } from '@lucide/svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { fetchAccountDetail, fetchAccounts } from '$lib/tauri';
  import {
    formatNumber,
    formatDate as fmtDate,
    formatSessionCurrency as fmt
  } from '$lib/utils/format.svelte';
  import { statusFor } from '$lib/utils/reconcile';
  import { errMsg } from '$lib/utils/error';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import { downloadCsv } from '$lib/utils/csv';
  import { fetchAllRows } from '$lib/utils/export';
  import type { AccountNode, DetailTransaction } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import BalanceTrendChart from '$lib/features/accounts/balance-trend-chart.svelte';
  import SubaccountsTable from '$lib/features/accounts/subaccounts-table.svelte';

  let guid = $derived(page.params.guid);
  let pageSize = $derived(appSettings.value.pageSize);
  let pg = $state(1);
  let sortBy = $state<'date' | 'description'>('date');
  let sortDir = $state<'asc' | 'desc'>('desc');

  // Filter state + URL-param restore must be set up before useAsyncData so the
  // first fetch already uses values from the URL.
  let searchInput = $state('');
  let dateFrom = $state('');
  let dateTo = $state('');

  let search = useDebouncedSearch(() => searchInput);

  useUrlParams([
    {
      key: 'page',
      get: () => (pg > 1 ? String(pg) : null),
      apply: (v) => {
        const n = Number(v);
        pg = Number.isInteger(n) && n >= 1 ? n : 1;
      }
    },
    {
      key: 'q',
      get: () => search.value || null,
      apply: (v) => {
        searchInput = v ?? '';
        search.set(v ?? '');
      },
      replace: true
    },
    {
      key: 'from',
      get: () => dateFrom || null,
      apply: (v) => (dateFrom = v ?? ''),
      replace: true
    },
    {
      key: 'to',
      get: () => dateTo || null,
      apply: (v) => (dateTo = v ?? ''),
      replace: true
    }
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    fetch: () => {
      const g = guid;
      if (!g) return Promise.reject(new Error('No account selected'));
      return fetchAccountDetail(
        g,
        pg,
        pageSize,
        dateFrom || undefined,
        dateTo || undefined,
        search.value || undefined,
        sortBy,
        sortDir
      );
    },
    label: 'Failed to load account transactions',
    onSuccess: (result) => {
      pg = result.page;
    }
  });

  let data = $derived(res.data);
  let loading = $derived(res.loading);
  let error = $derived(res.error);

  const accountsRes = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    fetch: () => fetchAccounts(),
    label: 'Failed to load sub-accounts'
  });

  function findNode(nodes: AccountNode[], g: string): AccountNode | undefined {
    for (const n of nodes) {
      if (n.guid === g) return n;
      const found = findNode(n.children, g);
      if (found) return found;
    }
    return undefined;
  }

  let subAccounts = $derived.by(() => {
    const trees = accountsRes.data?.trees;
    return guid && trees ? (findNode(trees, guid)?.children ?? []) : [];
  });

  let hasFilters = $derived(!!(searchInput || dateFrom || dateTo));

  let currentBalance = $derived(data?.account.balance ?? 0);

  function toggleSort(key: 'date' | 'description') {
    if (sortBy === key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortBy = key;
      sortDir = key === 'description' ? 'asc' : 'desc';
    }
  }

  function openTransaction(id: string) {
    void goto(resolve('/transactions/[id]', { id }));
  }

  function handlePageSizeChange(size: number) {
    saveSettings({ pageSize: size });
    pg = 1;
  }

  /**
   * Row-level navigation shortcut for plain clicks anywhere on the row.
   * Clicks on real links bubble up untouched: SvelteKit's root click listener
   * performs their client-side navigation (stopPropagation on the link would
   * defeat that interceptor and cause a full page reload).
   */
  function handleRowClick(e: MouseEvent, id: string) {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
      return;
    }
    if ((e.target as HTMLElement).closest('a')) return;
    openTransaction(id);
  }

  let prevNavKey = '';
  let prevSortKey = '';

  // Reset query state on account switch and page 1 on sort change. Declared
  // before the fetch effect so it runs first in the same flush.
  $effect(() => {
    if (!dbSession.value) {
      prevNavKey = '';
      return;
    }

    const g = guid ?? '';
    if (prevNavKey && g !== prevNavKey) {
      pg = 1;
      searchInput = '';
      search.set('');
      dateFrom = '';
      dateTo = '';
    }
    prevNavKey = g;

    const sk = `${sortBy}|${sortDir}`;
    if (prevSortKey && prevSortKey !== sk && pg !== 1) {
      pg = 1;
    }
    prevSortKey = sk;
  });

  $effect(() => {
    setBreadcrumb([
      { name: 'Accounts', href: resolve('/accounts') },
      { name: data?.account.description ?? data?.account.name ?? 'Account' }
    ]);
  });

  let exporting = $state(false);

  async function handleExportCsv() {
    const d = data;
    if (!d || !guid || exporting) return;
    exporting = true;
    try {
      const all = await fetchAllRows<DetailTransaction>(async (pageIdx) => {
        const r = await fetchAccountDetail(
          guid,
          pageIdx,
          500,
          dateFrom || undefined,
          dateTo || undefined,
          search.value || undefined,
          sortBy,
          sortDir
        );
        return { rows: r.transactions, total: r.total };
      });
      const headers = ['Date', 'Description', 'Number', 'Notes', 'Splits', 'Running Balance'];
      const rows = all.map((t) => [
        t.postDate,
        t.description ?? '',
        t.num ?? '',
        t.notes ?? '',
        t.splits.map((s) => `${s.accountName}: ${s.amount}`).join(' | '),
        String(t.runningBalance)
      ]);
      downloadCsv(`account-${guid}.csv`, headers, rows);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to export transactions'));
    } finally {
      exporting = false;
    }
  }
</script>

<svelte:head>
  <title>{data ? `${data.account.name} — Spendo` : 'Account — Spendo'}</title>
  <meta name="description" content="Account transactions and balance." />
</svelte:head>

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    {#if loading && !data}
      <div class="flex flex-col gap-1">
        <Skeleton class="h-8 w-48" />
        <Skeleton class="h-4 w-64" />
      </div>
    {:else if data}
      {@const d = data}
      <PageHeader title={d.account.description ?? d.account.name}>
        {#snippet description()}
          {d.account.accountType}
          {#if d.account.placeholder}· Placeholder{/if}
          {#if d.account.childrenCount > 0}· {d.account.childrenCount} sub-account{d.account
              .childrenCount === 1
              ? ''
              : 's'}{/if}
          {#if d.summary.totalTransactions > 0}· {formatNumber(d.summary.totalTransactions)} txn{d
              .summary.totalTransactions === 1
              ? ''
              : 's'}{/if}
        {/snippet}
        <Button
          variant="outline"
          onclick={handleExportCsv}
          disabled={d.transactions.length === 0 || exporting}
        >
          {#if exporting}
            <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
          {:else}
            <Download aria-hidden="true" class="size-3" />
          {/if}
          Export
        </Button>
      </PageHeader>
    {/if}

    {#if loading && !data}
      <KpiCardRow />
    {:else if data}
      {@const d = data}
      <div class="grid grid-cols-1 gap-6 sm:grid-cols-3">
        <KpiCard title="Total Balance" value={fmt(currentBalance)}>
          {#snippet icon()}
            <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Income" value={fmt(d.summary.totalIncome)} valueClass="text-success">
          {#snippet icon()}
            <TrendingUp aria-hidden="true" class="size-4 text-success" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Expenses"
          value={fmt(d.summary.totalExpenses)}
          valueClass="text-destructive"
        >
          {#snippet icon()}
            <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
          {/snippet}
        </KpiCard>
      </div>
    {/if}

    <!-- Sub-accounts -->
    {#if subAccounts.length > 0}
      <DimWrapper active={accountsRes.loading}>
        <section class="flex flex-col gap-3">
          <h2 class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
            Sub-accounts
          </h2>
          <SubaccountsTable nodes={subAccounts} />
        </section>
      </DimWrapper>
    {/if}

    <!-- Balance over time -->
    {#if guid}
      <BalanceTrendChart {guid} />
    {/if}

    <!-- Filters + action buttons -->
    <div class="flex flex-wrap items-end gap-2">
      <SearchInput
        id="acct-search"
        label="Search description, notes, memo, number"
        placeholder="Search description, notes, memo, number..."
        bind:value={searchInput}
        class="grow basis-48"
      />
      <div class="flex flex-col gap-1">
        <label for="acct-from" class="text-xs text-muted-foreground">From</label>
        <DatePicker id="acct-from" bind:value={dateFrom} placeholder="Start date" />
      </div>
      <div class="flex flex-col gap-1">
        <label for="acct-to" class="text-xs text-muted-foreground">To</label>
        <DatePicker id="acct-to" bind:value={dateTo} placeholder="End date" />
      </div>
      {#if hasFilters}
        <Button
          variant="ghost"
          onclick={() => {
            searchInput = '';
            search.set('');
            dateFrom = '';
            dateTo = '';
          }}
        >
          <X aria-hidden="true" class="mr-1 size-3" />
          Clear
        </Button>
      {/if}
    </div>

    {#if loading && !data}
      <TableSkeleton cols={4} rows={5} />
    {:else if error}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {:else if data}
      {@const d = data}

      {#if d.transactions.length === 0}
        <EmptyState
          card
          title="No transactions"
          description="No transactions found{search.value || dateFrom || dateTo
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
          <TableShell>
            <Table.Header>
              <Table.Row class="bg-muted/75">
                <SortHead
                  label="Date"
                  active={sortBy === 'date'}
                  dir={sortDir}
                  onclick={() => toggleSort('date')}
                />
                <SortHead
                  label="Description"
                  active={sortBy === 'description'}
                  dir={sortDir}
                  onclick={() => toggleSort('description')}
                />
                <Table.Head class="text-muted-foreground">Transfer</Table.Head>
                <Table.Head class="text-muted-foreground">Status</Table.Head>
                <Table.Head class="text-right text-muted-foreground">Running Balance</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each d.transactions as txn (txn.guid)}
                <Table.Row
                  class="cursor-pointer hover:bg-muted/50"
                  onclick={(e) => handleRowClick(e, txn.guid)}
                >
                  <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground">
                    {fmtDate(txn.postDate)}
                  </Table.Cell>
                  <Table.Cell class="max-w-md py-2.5 font-medium">
                    <!-- Accessible link target for keyboard/middle-click; the
                         row click is a convenience shortcut. Its event must
                         keep bubbling so SvelteKit's router can intercept it. -->
                    <a
                      class="block truncate"
                      href={resolve('/transactions/[id]', { id: txn.guid })}
                      title={txn.description ?? undefined}
                    >
                      {#if txn.num}<span class="mr-1 text-muted-foreground">#{txn.num}</span>{/if}
                      {txn.description || '\u2014'}
                    </a>
                    {#if txn.notes}
                      <p
                        class="truncate text-xs font-normal text-muted-foreground"
                        title={txn.notes}
                      >
                        {txn.notes}
                      </p>
                    {/if}
                  </Table.Cell>
                  <Table.Cell
                    class="max-w-xs truncate py-2.5 text-muted-foreground"
                    title={txn.counterAccounts.join(', ')}
                  >
                    {txn.counterAccounts.length > 0 ? txn.counterAccounts.join(', ') : '\u2014'}
                  </Table.Cell>
                  <Table.Cell class="py-2.5">
                    {@const status = statusFor(txn.reconcileState)}
                    <Badge variant={status.variant}>{status.label}</Badge>
                  </Table.Cell>
                  <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
                    {fmt(txn.runningBalance)}
                  </Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </TableShell>

          <PaginationNav
            bind:page={pg}
            totalPages={d.totalPages}
            pageSize={appSettings.value.pageSize}
            onPageSizeChange={handlePageSizeChange}
          />
        </DimWrapper>
      {/if}
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Account" />
    <ImportPrompt />
  </div>
{/if}
