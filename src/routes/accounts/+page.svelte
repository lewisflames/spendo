<script lang="ts">
  import { Landmark, PiggyBank, CreditCard, TrendingUp, Plus } from '@lucide/svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import AccountDialog from '$lib/features/accounts/account-dialog.svelte';
  import {
    CATEGORY_ORDER,
    CATEGORY_LABEL_PLURAL as CATEGORY_LABEL,
    nodeCategory
  } from '$lib/features/accounts/account-types';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useUrlParams, dateParam } from '$lib/hooks/use-url-params.svelte';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import { AsOfPicker } from '$lib/components/shared/as-of-picker';
  import FutureAsOfNote from '$lib/components/shared/future-as-of-note.svelte';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import { clampZero, formatSessionCurrency, signColor, plural } from '$lib/utils/format.svelte';
  import AccountTable from '$lib/features/accounts/account-table.svelte';
  import BalanceTree from '$lib/features/accounts/balance-tree.svelte';

  type Category = 'ASSET' | 'EQUITY' | 'LIABILITY';

  const KPI_CATEGORIES: Category[] = ['ASSET', 'EQUITY', 'LIABILITY'];

  const KPI_ICONS: Record<Category, typeof Landmark> = {
    ASSET: Landmark,
    EQUITY: PiggyBank,
    LIABILITY: CreditCard
  };

  const KPI_COLORS: Record<Category, string> = {
    ASSET: 'text-success',
    EQUITY: 'text-warning',
    LIABILITY: 'text-destructive'
  };

  let asOf = $state(todayStr());
  let dialogOpen = $state(false);

  useUrlParams([
    dateParam(
      'asOf',
      () => (asOf === todayStr() ? null : asOf || null),
      (v) => (asOf = v ?? todayStr())
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!asOf,
    fetch: () => fetchAccounts(asOf || undefined),
    label: 'Failed to load accounts'
  });

  // Top-level accounts only, in the canonical category order (unknown
  // categories sort last).
  let accounts = $derived.by(() => {
    const order = new Map(CATEGORY_ORDER.map((c, i) => [c, i]));
    return [...(res.data?.trees ?? [])].sort(
      (a, b) =>
        (order.get(nodeCategory(a.accountType)) ?? CATEGORY_ORDER.length) -
        (order.get(nodeCategory(b.accountType)) ?? CATEGORY_ORDER.length)
    );
  });

  function categoryTotal(cat: Category): number {
    return res.data?.byType[cat]?.totalBalance ?? 0;
  }

  let kpiCards = $derived.by(() => {
    if (!res.data) return [];
    return KPI_CATEGORIES.map((cat) => ({
      key: cat,
      title: CATEGORY_LABEL[cat],
      value: categoryTotal(cat),
      icon: KPI_ICONS[cat],
      iconClass: KPI_COLORS[cat],
      valueClass: KPI_COLORS[cat]
    }));
  });

  let retainedEarnings = $derived(res.data?.retainedEarnings ?? 0);
  let balanced = $derived(res.data !== null && Math.abs(res.data?.balanceCheck ?? 0) < 0.005);

  function displayValue(n: number): string {
    return formatSessionCurrency(clampZero(n));
  }
</script>

<svelte:head>
  <title>Accounts — Spendo</title>
  <meta name="description" content="Chart of accounts." />
</svelte:head>

<BookGate title="Accounts" description="Chart of accounts">
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Accounts"
      description={res.data
        ? `Chart of accounts · ${plural(res.data.totalAccounts, 'account')}`
        : ''}
    >
      <div class="flex items-center gap-3">
        {#if dbSession.value}
          <Button onclick={() => (dialogOpen = true)}>
            <Plus aria-hidden="true" class="size-4" />
            New account
          </Button>
        {/if}
        <AsOfPicker bind:value={asOf} />
      </div>
    </PageHeader>

    <AccountDialog
      bind:open={dialogOpen}
      mode="create"
      onSaved={() => {
        res.reload();
        toast.success('Account created');
      }}
      onError={(msg) => toast.error(msg)}
    />

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="accounts" />
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if res.data}
      <!-- Summary cards by account category -->
      <DimWrapper active={res.loading} class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
        {#each kpiCards as card (card.key)}
          <KpiCard title={card.title} value={displayValue(card.value)} valueClass={card.valueClass}>
            {#snippet icon()}
              {@const Icon = card.icon}
              <Icon aria-hidden="true" class="size-4 {card.iconClass}" />
            {/snippet}
          </KpiCard>
        {/each}
        <KpiCard
          title="Retained Earnings"
          value={displayValue(retainedEarnings)}
          valueClass={signColor(retainedEarnings)}
        >
          {#snippet icon()}
            <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
      </DimWrapper>

      <MissingFxWarning
        missing={res.data.missingFx}
        missingPrices={res.data.missingPrices}
        reportCurrency={res.data.reportCurrency}
      />

      <!-- Balance composition tree -->
      <BalanceTree data={res.data?.trees ?? []} loading={res.loading} />

      {#if accounts.length === 0}
        <EmptyState card title="No accounts found" description="No top-level accounts exist yet.">
          {#snippet icon()}
            <Landmark aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
        </EmptyState>
      {:else}
        <DimWrapper active={res.loading}>
          <AccountTable rows={accounts} />
        </DimWrapper>
      {/if}

      {#if !res.loading && !res.error}
        <FutureAsOfNote {asOf} />
        <p class="text-xs text-muted-foreground">
          {#if balanced}
            Balanced · Assets = Liabilities + Equity (incl. unrealized gains) + Retained earnings.
          {:else}
            Out of balance by {formatSessionCurrency(res.data.balanceCheck)} — usually a missing FX rate,
            an unpriced security, or currency translation in a book without trading accounts.
          {/if}
        </p>
      {/if}
    {/if}
  </div>
</BookGate>
