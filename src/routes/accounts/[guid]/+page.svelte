<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Pencil, Plus } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import AccountDialog, { type AccountInitial } from '$lib/features/accounts/account-dialog.svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { fetchAccountDetail } from '$lib/tauri';
  import { clampZero, sessionCurrency } from '$lib/utils/format.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import {
    useUrlParams,
    pageParam,
    debouncedSearchParam,
    dateParam
  } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
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

  let editOpen = $state(false);
  let createOpen = $state(false);

  let editInitial = $derived.by((): AccountInitial | null => {
    const a = data?.account;
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
  let subAccounts = $derived(data?.account.childAccounts ?? []);

  let currentBalance = $derived(data?.account.balance ?? 0);

  let subtreeNode = $derived(data?.subtree ?? null);
  let parentInfo = $derived(data?.parent ?? null);
  let hasPieData = $derived(
    !!subtreeNode && subtreeNode.children.some((c) => Math.abs(clampZero(c.balance)) >= 0.005)
  );

  // Charts loading gate: while the detail payload hasn't arrived we reserve
  // both chart slots (assuming the pie exists) so the trend chart settling
  // before/after the pie data never changes the grid shape mid-paint.
  let chartsLoading = $derived((loading || dbBootstrapping.value) && !data);
  let showPie = $derived(!chartsLoading && hasPieData);

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
</script>

<svelte:head>
  <title>{data ? `${data.account.name} — Spendo` : 'Account — Spendo'}</title>
  <meta name="description" content="Account transactions and balance." />
</svelte:head>

{#if dbSession.value || dbBootstrapping.value}
  <div class="flex flex-col gap-6">
    {#if (loading || dbBootstrapping.value) && !data}
      <PageHeaderSkeleton />
    {:else if data}
      <AccountDetailHeader account={data.account} summary={data.summary} {isToday} bind:asOf>
        {#if dbSession.value}
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
          guid={data.account.guid}
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
          parentPreset={data.account.guid}
          onSaved={() => {
            res.reload();
            toast.success('Account created');
          }}
          onError={(msg) => toast.error(msg)}
        />
      {/if}
    {/if}

    {#if (loading || dbBootstrapping.value) && !data}
      <KpiCardRow />
    {:else if data}
      <AccountKpiCards summary={data.summary} {currentBalance} {isToday} {asOf} />
    {/if}

    <!-- Balance over time + composition. The grid keeps its shape across the
      detail fetch: the pie slot is reserved with a skeleton while loading,
      and the trend chart instance is never remounted by the loading/settle
      transition, so independent fetch timing can't shift layout. -->
    {#if guid}
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div class={showPie || chartsLoading ? 'lg:col-span-2' : 'lg:col-span-3'}>
          {#if dbSession.value}
            <BalanceTrendChart {guid} accountType={data?.account.accountType} {asOf} />
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
          <BalancePie node={subtreeNode} parent={parentInfo} {loading} />
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
      {data}
      {loading}
      {error}
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
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Account" />
    <ImportPrompt />
  </div>
{/if}
