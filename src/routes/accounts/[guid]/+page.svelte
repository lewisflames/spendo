<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { Button } from '$lib/components/ui/button';
  import {
    TrendingUp,
    TrendingDown,
    Wallet,
    Receipt,
    Download,
    LoaderCircle,
    Columns3
  } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { fetchAccountDetail } from '$lib/tauri';
  import {
    clampZero,
    formatSessionCurrency as fmt,
    formatDate,
    sessionCurrency,
    plural
  } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import { downloadCsv } from '$lib/utils/csv';
  import { fetchAllRows } from '$lib/utils/export';
  import type { DetailTransaction } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import {
    useUrlParams,
    pageParam,
    debouncedSearchParam,
    dateParam
  } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import TransactionFilters from '$lib/features/transactions/transaction-filters.svelte';
  import TransactionTable from '$lib/features/transactions/transaction-table.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageHeaderSkeleton from '$lib/components/shared/page-header-skeleton.svelte';
  import { AsOfPicker } from '$lib/components/shared/as-of-picker';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import BalanceTrendChart from '$lib/features/accounts/balance-trend-chart.svelte';
  import BalancePie from '$lib/features/accounts/balance-pie.svelte';
  import AccountTable from '$lib/features/accounts/account-table.svelte';

  let guid = $derived(page.params.guid);
  let pageSize = $derived(appSettings.value.pageSize);
  let pg = $state(1);
  const sort = useTableSort<'date' | 'description' | 'amount'>(
    { sortBy: 'date', sortDir: 'desc' },
    ['date', 'amount']
  );
  let sortBy = $derived(sort.sortBy);
  let sortDir = $derived(sort.sortDir);

  type ColumnId = 'transfer' | 'status' | 'amount' | 'runningBalance';

  const COLUMNS: { id: ColumnId; label: string }[] = [
    { id: 'transfer', label: 'Transfer' },
    { id: 'status', label: 'Status' },
    { id: 'amount', label: 'Amount' },
    { id: 'runningBalance', label: 'Balance' }
  ];

  const DEFAULT_COLUMNS: ColumnId[] = ['transfer', 'amount'];

  const cols = useColumnVisibility<ColumnId>(DEFAULT_COLUMNS);
  let visibleColumns = $derived(cols.visibleColumns);

  let skeletonCols = $derived(visibleColumns.length + 2);

  let currency = $derived(sessionCurrency());

  // Filter state + URL-param restore must be set up before useAsyncData so the
  // first fetch already uses values from the URL.
  let searchInput = $state('');
  let dateFrom = $state('');
  let dateTo = $state('');
  // Point-in-time view for balances (header/KPIs/children/pie/trend). The
  // transaction ledger keeps its own From/To filters independently.
  let asOf = $state(todayStr());

  let search = useDebouncedSearch(() => searchInput);

  useUrlParams([
    pageParam(
      'page',
      () => pg,
      (p) => (pg = p)
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
    dateParam(
      'asOf',
      () => (asOf === todayStr() ? null : asOf || null),
      (v) => (asOf = v ?? todayStr())
    )
  ]);

  // As-of view state for balances. A future as-of silently shows current
  // balances backend-side (future txs excluded), so disclose like /accounts.
  let isToday = $derived(!asOf || asOf === todayStr());
  let isFutureAsOf = $derived(!!asOf && asOf > todayStr());

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
        sortDir,
        asOf || undefined
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

  // Direct children come with the detail payload — no second full-tree fetch.
  let subAccounts = $derived(data?.account.childAccounts ?? []);

  let currentBalance = $derived(data?.account.balance ?? 0);

  let subtreeNode = $derived(data?.subtree ?? null);
  let parentInfo = $derived(data?.parent ?? null);
  let hasPieData = $derived(
    !!subtreeNode && subtreeNode.children.some((c) => Math.abs(clampZero(c.balance)) >= 0.005)
  );

  function handlePageSizeChange(size: number) {
    saveSettings({ pageSize: size });
    pg = 1;
  }

  let prevNavKey = '';
  let prevSortKey = '';

  // Reset query state on account switch and page 1 on sort change. Declared
  // before the fetch effect so it runs first in the same flush. `asOf` is
  // intentionally left URL-driven (entering from the list starts at today).
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
      const headers = [
        'Date',
        'Description',
        'Number',
        'Notes',
        'Splits',
        'Amount',
        'Running Balance'
      ];
      const rows = all.map((t) => [
        t.postDate,
        t.description ?? '',
        t.num ?? '',
        t.notes ?? '',
        t.splits.map((s) => `${s.accountName}: ${s.amount}`).join(' | '),
        String(t.amount),
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
      <PageHeaderSkeleton />
    {:else if data}
      {@const d = data}
      <PageHeader title={d.account.description ?? d.account.name}>
        {#snippet description()}
          {d.account.accountType}
          {#if d.account.placeholder}· Placeholder{/if}
          {#if d.account.descendantCount > 0}
            · {plural(d.account.descendantCount, 'sub-account')}
          {/if}
          {#if d.summary.totalTransactions > 0}
            · {plural(d.summary.totalTransactions, 'txn')}
          {/if}
          {#if !isToday}
            · as of {formatDate(asOf)}
          {/if}
        {/snippet}
        <AsOfPicker bind:value={asOf} />
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
      {#if !isToday}
        <p class="text-xs text-muted-foreground">
          Balances as of {formatDate(asOf)} — the transaction list below keeps its own From/To filters.
        </p>
      {/if}
      {#if isFutureAsOf}
        <p class="text-xs text-muted-foreground">
          Future date selected — future transactions are excluded, so this shows current balances.
        </p>
      {/if}
    {/if}

    <!-- Balance over time + composition -->
    {#if guid}
      {#if hasPieData && subtreeNode}
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
          <div class="lg:col-span-2">
            <BalanceTrendChart {guid} accountType={data?.account.accountType} {asOf} />
          </div>
          <BalancePie node={subtreeNode} parent={parentInfo} {loading} />
        </div>
      {:else}
        <BalanceTrendChart {guid} accountType={data?.account.accountType} {asOf} />
      {/if}
    {/if}

    <!-- Sub-accounts -->
    {#if subAccounts.length > 0}
      <AccountTable rows={subAccounts} showType nameHeader="Subaccount" />
    {/if}

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
            <DropdownMenu.Item onclick={() => cols.resetColumns()}
              >Reset to default</DropdownMenu.Item
            >
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      {/snippet}
    </TransactionFilters>

    {#if loading && !data}
      <TableSkeleton cols={skeletonCols} rows={5} />
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
          <TransactionTable
            transactions={d.transactions}
            {currency}
            sortBy={sort.sortBy}
            sortDir={sort.sortDir}
            onSort={(key) => sort.toggle(key)}
            variant="account"
            {visibleColumns}
          />

          <PaginationNav
            bind:page={pg}
            totalPages={d.totalPages}
            totalItems={d.total}
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
