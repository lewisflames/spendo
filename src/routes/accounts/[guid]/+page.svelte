<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Pencil, Plus, CircleCheck } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import AccountDialog, { type AccountInitial } from '$lib/features/accounts/account-dialog.svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { fetchAccountDetail } from '$lib/tauri';
  import { clampZero, sessionCurrency } from '$lib/utils/format.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import {
    useUrlParams,
    pageParam,
    debouncedSearchParam,
    dateParam
  } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useResetPageOnChange } from '$lib/hooks/use-page-reset.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import PageHeaderSkeleton from '$lib/components/shared/page-header-skeleton.svelte';
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import BalanceTrendChart from '$lib/features/accounts/balance-trend-chart.svelte';
  import BalancePie from '$lib/features/accounts/balance-pie.svelte';
  import AccountTable from '$lib/features/accounts/account-table.svelte';
  import AccountDetailHeader from '$lib/features/accounts/account-detail-header.svelte';
  import AccountKpiCards from '$lib/features/accounts/account-kpi-cards.svelte';
  import AccountLedger from '$lib/features/accounts/account-ledger.svelte';
  import { isReconcilable } from '$lib/features/reconcile/math';

  let guid = $derived(page.params.guid);
  let pageSize = $derived(appSettings.value.pageSize);
  // `pg`, not `page`: the $app/state `page` import above owns that name.
  let pg = $state(1);
  const sort = useTableSort<'date' | 'description' | 'amount'>(
    { sortBy: 'date', sortDir: 'desc' },
    ['date', 'amount']
  );
  let sortBy = $derived(sort.sortBy);
  let sortDir = $derived(sort.sortDir);

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

  // As-of view state for balances.
  let isToday = $derived(!asOf || asOf === todayStr());

  // Reset query state on account switch and page 1 on sort change. Runs
  // before the fetch effect in the same flush, so the fetch sees the reset
  // page immediately. `asOf` is intentionally left URL-driven (entering from
  // the list starts at today).
  useResetPageOnChange(
    () => guid ?? '',
    () => {
      pg = 1;
      searchInput = '';
      search.set('');
      dateFrom = '';
      dateTo = '';
    }
  );
  useResetPageOnChange(
    () => `${sortBy}|${sortDir}`,
    () => {
      pg = 1;
    }
  );

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

  let editOpen = $state(false);
  let createOpen = $state(false);

  let editInitial = $derived.by((): AccountInitial | null => {
    const a = res.data?.account;
    if (!a) return null;
    return {
      name: a.name,
      accountType: a.accountType,
      parentGuid: a.parentGuid,
      code: a.code,
      description: a.description,
      commodityGuid: a.commodityGuid,
      placeholder: a.placeholder
    };
  });

  // Direct children come with the detail payload — no second full-tree fetch.
  let subAccounts = $derived(res.data?.account.childAccounts ?? []);

  let currentBalance = $derived(res.data?.account.balance ?? 0);

  let subtreeNode = $derived(res.data?.subtree ?? null);
  let parentInfo = $derived(res.data?.parent ?? null);
  let hasPieData = $derived(
    !!subtreeNode && subtreeNode.children.some((c) => Math.abs(clampZero(c.balance)) >= 0.005)
  );

  // Charts loading gate: while the detail payload hasn't arrived we reserve
  // both chart slots (assuming the pie exists) so the trend chart settling
  // before/after the pie data never changes the grid shape mid-paint.
  let chartsLoading = $derived((res.loading || dbBootstrapping.value) && !res.data);
  let showPie = $derived(!chartsLoading && hasPieData);

  $effect(() => {
    // Track navigation explicitly so the crumb refreshes even when the
    // payload is unchanged; title-bar falls back to the route title otherwise.
    void page.url.pathname;
    setBreadcrumb([
      { name: 'Accounts', href: resolve('/accounts') },
      { name: res.data?.account.description ?? res.data?.account.name ?? 'Account' }
    ]);
  });
</script>

<svelte:head>
  <title>{res.data ? `${res.data.account.name} — Spendo` : 'Account — Spendo'}</title>
  <meta name="description" content="Account transactions and balance." />
</svelte:head>

<BookGate title="Account">
  <div class="flex flex-col gap-6">
    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageHeaderSkeleton />
    {:else if res.data}
      <AccountDetailHeader
        account={res.data.account}
        summary={res.data.summary}
        {isToday}
        bind:asOf
      >
        {#if dbSession.value}
          {#if guid && res.data && isReconcilable(res.data.account.accountType, res.data.account.placeholder)}
            <Button variant="outline" size="sm" href={resolve('/reconcile/[guid]', { guid })}>
              <CircleCheck aria-hidden="true" class="size-3" />
              Reconcile
            </Button>
          {/if}
          <Button variant="outline" size="sm" onclick={() => (createOpen = true)}>
            <Plus aria-hidden="true" class="size-3" />
            Sub-account
          </Button>
          <Button variant="outline" size="sm" onclick={() => (editOpen = true)}>
            <Pencil aria-hidden="true" class="size-3" />
            Edit
          </Button>
        {/if}
      </AccountDetailHeader>
      {#if guid}
        <AccountDialog
          bind:open={editOpen}
          mode="edit"
          guid={res.data.account.guid}
          initial={editInitial}
          onSaved={() => {
            res.reload();
            toast.success('Account updated');
          }}
          onDeleted={() => {
            toast.success('Account deleted');
            void goto(resolve('/accounts'));
          }}
          onError={(msg) => toast.error(msg)}
        />
        <AccountDialog
          bind:open={createOpen}
          mode="create"
          parentPreset={res.data.account.guid}
          onSaved={() => {
            res.reload();
            toast.success('Account created');
          }}
          onError={(msg) => toast.error(msg)}
        />
      {/if}
    {/if}

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <KpiCardRow />
    {:else if res.data}
      <AccountKpiCards summary={res.data.summary} {currentBalance} {isToday} {asOf} />
    {/if}

    <!-- Balance over time + composition. The grid keeps its shape across the
      detail fetch: the pie slot is reserved with a skeleton while loading,
      and the trend chart instance is never remounted by the loading/settle
      transition, so independent fetch timing can't shift layout. -->
    {#if guid}
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div class={showPie || chartsLoading ? 'lg:col-span-2' : 'lg:col-span-3'}>
          {#if dbSession.value}
            <BalanceTrendChart {guid} accountType={res.data?.account.accountType} {asOf} />
          {:else}
            <Card.Root>
              <Card.Header>
                <Skeleton class="h-6 w-40" />
                <Skeleton class="h-4 w-56" />
              </Card.Header>
              <Card.Content>
                <Skeleton class="h-72 w-full" />
              </Card.Content>
            </Card.Root>
          {/if}
        </div>
        {#if showPie}
          <BalancePie node={subtreeNode} parent={parentInfo} loading={res.loading} />
        {:else if chartsLoading}
          <BalancePie node={null} parent={null} loading={true} />
        {/if}
      </div>
    {/if}

    <!-- Sub-accounts -->
    {#if subAccounts.length > 0}
      <AccountTable rows={subAccounts} showType nameHeader="Subaccount" />
    {/if}

    <AccountLedger
      data={res.data}
      loading={res.loading}
      error={res.error}
      bind:searchInput
      bind:dateFrom
      bind:dateTo
      searchText={search.value}
      {sortBy}
      {sortDir}
      onSort={(key) => sort.toggle(key)}
      {currency}
      bind:page={pg}
      onPageSizeChange={(size) => applyPageSize(size, (v) => (pg = v))}
      onRetry={() => res.reload()}
    />
  </div>
</BookGate>
