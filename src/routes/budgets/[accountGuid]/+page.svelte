<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Table from '$lib/components/ui/table';
  import { cn } from '$lib/utils';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import {
    sessionCurrency,
    formatSessionCurrency,
    formatSignedCurrency,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import { getBudgetDetail, getBudgetDateRange, deleteBudgetYear, fetchAccounts } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useUrlParams, yearParam } from '$lib/hooks/use-url-params.svelte';
  import { Pencil, Trash2 } from '@lucide/svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import BudgetEntryDialog from '$lib/features/budgets/budget-entry-dialog.svelte';
  import BudgetYearStepper from '$lib/features/budgets/budget-year-stepper.svelte';
  import PacingBar from '$lib/features/budgets/pacing-bar.svelte';
  import {
    accountNameMap,
    budgetYearBounds,
    buildYearOptions,
    fallbackYearOptions,
    clampYear as clampYearToRange,
    remainingColor
  } from '$lib/features/budgets/budget-utils';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';

  let accountGuid = $derived(page.params.accountGuid);

  const now = new Date();
  let yearSel = $state(String(now.getFullYear()));
  let selectedYear = $derived(Number(yearSel));

  // URL-param restore must be set up before useAsyncData so the first fetch
  // already uses the year from the URL.
  useUrlParams([
    yearParam(
      'year',
      () => yearSel,
      (v) => (yearSel = v)
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!accountGuid,
    fetch: async () => {
      const g = accountGuid;
      if (!g) return Promise.reject(new Error('No account selected'));
      const [detail, range, accounts] = await Promise.all([
        getBudgetDetail(g, selectedYear),
        getBudgetDateRange(),
        fetchAccounts()
      ]);
      return { detail, range, accounts };
    },
    label: 'Failed to load budget',
    onSuccess: () => clampSelection()
  });

  // Previous data is kept by useAsyncData while refetching: only accept the
  // detail when it matches the selected year, so switching to a year without
  // a budget falls through to the empty state instead of showing stale data.
  let detail = $derived(
    res.data?.detail && res.data.detail.year === selectedYear ? res.data.detail : null
  );
  let dateRange = $derived(res.data?.range ?? null);
  let accountsData = $derived(res.data?.accounts ?? null);
  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let formError = $state<string | null>(null);

  let showEditDialog = $state(false);
  let showDeleteConfirm = $state(false);

  let budget = $derived(detail?.budget ?? null);
  let summary = $derived(detail?.summary ?? null);
  let monthlyEntries = $derived(detail?.monthlyEntries ?? []);
  let currency = $derived(sessionCurrency());
  const fmt = formatSessionCurrency;

  // Account display name for years without a budget row (resolved via accounts).
  let accountNames = $derived(accountNameMap(accountsData?.trees));

  $effect(() => {
    setBreadcrumb([
      { name: 'Budgets', href: resolve('/budgets') },
      { name: budget?.accountDescription ?? budget?.accountName ?? 'Budget' }
    ]);
  });

  let accountName = $derived(
    budget?.accountDescription ??
      budget?.accountName ??
      (accountGuid ? accountNames[accountGuid] : undefined) ??
      'Budget'
  );

  let { minYear, maxYear } = $derived.by(() => {
    const { minYear, maxYear: rawMax } = budgetYearBounds(dateRange);
    // Detail page extends one year beyond the latest transaction so next
    // year's budget can be planned ahead (mirrors BudgetYearDialog).
    return { minYear, maxYear: rawMax !== null ? rawMax + 1 : null };
  });

  let yearOptions = $derived.by(() => {
    if (minYear === null || maxYear === null) {
      return fallbackYearOptions();
    }
    return buildYearOptions(minYear, maxYear);
  });

  function clampYear(y: number): number {
    return clampYearToRange(y, minYear, maxYear);
  }

  function clampSelection() {
    if (minYear === null || maxYear === null) return;
    const y = clampYear(selectedYear);
    if (y !== selectedYear) yearSel = String(y);
  }

  let atMin = $derived(minYear !== null && selectedYear <= minYear);
  let atMax = $derived(maxYear !== null && selectedYear >= maxYear);

  function shiftYear(delta: number) {
    yearSel = String(clampYear(selectedYear + delta));
  }

  function monthLabel(m: number): string {
    return new Date(2000, m - 1, 1).toLocaleDateString(appSettings.value.locale, {
      month: 'long'
    });
  }

  function handleSaveSuccess(message = 'Annual budget saved') {
    res.reload();
    formError = null;
    toast.success(message);
  }

  async function confirmDelete() {
    if (!accountGuid) return;
    try {
      await deleteBudgetYear(accountGuid, selectedYear);
      showDeleteConfirm = false;
      toast.success('Budget deleted');
      // The viewed year no longer has a budget — back to the list.
      // eslint-disable-next-line svelte/no-navigation-without-resolve
      goto(resolve('/budgets') + `?year=${selectedYear}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to delete budget'));
    }
  }
</script>

<svelte:head>
  <title>{accountName} — Budgets — Spendo</title>
</svelte:head>

{#snippet headerDescription()}
  <span class="flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
    Yearly budget for this account
  </span>
{/snippet}

{#snippet loadingSkeleton()}
  <KpiCardRow count={4} />
  <!-- Skeleton mirrors the pacing card + monthly table shape. -->
  <div class="rounded-lg border p-4">
    <div class="flex flex-wrap items-baseline justify-between gap-2">
      <Skeleton class="h-4 w-52" />
      <Skeleton class="h-4 w-44" />
    </div>
    <div class="mt-3 flex items-center gap-3">
      <Skeleton class="h-2 flex-1" />
      <Skeleton class="h-4 w-12" />
    </div>
    <Skeleton class="mt-2 h-3 w-72" />
  </div>
  <TableSkeleton cols={6} rows={6} />
{/snippet}

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader title={accountName} description={headerDescription}>
      <div class="flex flex-wrap items-center gap-2">
        <BudgetYearStepper
          bind:yearSel
          {yearOptions}
          {atMin}
          {atMax}
          onPrev={() => shiftYear(-1)}
          onNext={() => shiftYear(1)}
        />
        <Button
          variant="outline"
          onclick={() => (showEditDialog = true)}
          disabled={detail === null && loading}
        >
          <Pencil aria-hidden="true" />
          Set target
        </Button>
        <Button variant="destructive" onclick={() => (showDeleteConfirm = true)} disabled={!detail}>
          <Trash2 aria-hidden="true" />
          Delete
        </Button>
      </div>
    </PageHeader>

    {#if formError}
      <ErrorCard message={formError} />
    {/if}
    {#if error && detail !== null}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {/if}
    {#if detail !== null && summary !== null}
      <DimWrapper active={loading} class="flex flex-col gap-6">
        <!-- Yearly summary -->
        <div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
          <KpiCard title="Annual target" value={fmt(detail.amount)} valueClass="text-chart-1" />
          <KpiCard
            title="Carry-in (Jan 1)"
            value={formatSignedCurrency(summary.rolloverBalance, currency)}
            valueClass={cn(summary.rolloverBalance < 0 ? 'text-destructive' : 'text-success')}
          />
          <KpiCard
            title="Spent (YTD)"
            value={fmt(summary.actualAmount)}
            valueClass={cn(
              summary.actualAmount > summary.effectiveTotal ? 'text-destructive' : 'text-success'
            )}
          />
          <KpiCard
            title="Remaining"
            value={formatSignedCurrency(summary.remaining, currency)}
            valueClass={remainingColor(summary.remaining)}
          />
        </div>

        <!-- Pacing vs expected YTD -->
        <div class="rounded-lg border p-4">
          <div class="flex flex-wrap items-baseline justify-between gap-2">
            <h2 class="text-sm font-medium">
              Pacing vs expected YTD
              <span class="font-normal text-muted-foreground">
                ({summary.elapsedMonths}/12 months elapsed)
              </span>
            </h2>
            <p class="text-sm text-muted-foreground tabular-nums">
              {fmt(summary.actualAmount)} of {fmt(summary.expectedYtd)} expected
              {#if summary.elapsedMonths > 0}
                ·
                <span class={remainingColor(summary.delta)}>
                  {#if summary.delta >= 0}
                    {fmt(summary.delta)} under
                  {:else}
                    {fmt(-summary.delta)} over
                  {/if}
                </span>
              {/if}
            </p>
          </div>
          {#if summary.elapsedMonths > 0}
            <div class="mt-3 flex items-center gap-3">
              <PacingBar
                pacingPercentage={summary.pacingPercentage}
                actualAmount={summary.actualAmount}
                expectedYtd={summary.expectedYtd}
                delta={summary.delta}
                {fmt}
                size="md"
                tooltip={false}
              />
            </div>
            <p class="mt-2 text-xs text-muted-foreground">
              Full-year progress: {Math.max(0, summary.percentage).toFixed(1)}% of {fmt(
                summary.effectiveTotal
              )} effective total. Carry-in counts in full from January.
            </p>
          {:else}
            <p class="mt-2 text-sm text-muted-foreground">Future year — no pacing yet.</p>
          {/if}
        </div>

        <!-- Monthly pacing breakdown (read-only) -->
        <TableShell>
          <Table.Header>
            <Table.Row class="bg-muted/75">
              <Table.Head class="text-muted-foreground">Month</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Spent</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Cumulative</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Expected</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Ahead / Behind</Table.Head>
              <Table.Head class="text-muted-foreground">Pacing</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each monthlyEntries as e (e.month)}
              {@const isFuture = summary.elapsedMonths < 12 && e.month > summary.elapsedMonths}
              <Table.Row>
                <Table.Cell class="py-2.5 capitalize">{monthLabel(e.month)}</Table.Cell>
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if isFuture}
                    <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
                  {:else}
                    {fmt(e.actualAmount)}
                  {/if}
                </Table.Cell>
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {#if isFuture}
                    <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
                  {:else}
                    {fmt(e.cumulativeActual)}
                  {/if}
                </Table.Cell>
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {fmt(e.cumulativeExpected)}
                </Table.Cell>
                <Table.Cell
                  class={cn('py-2.5 text-right tabular-nums', !isFuture && remainingColor(e.delta))}
                >
                  {#if isFuture}
                    <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
                  {:else}
                    {formatSignedCurrency(e.delta, currency)}
                  {/if}
                </Table.Cell>
                <Table.Cell class="py-2.5">
                  {#if isFuture}
                    <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
                  {:else}
                    <PacingBar
                      pacingPercentage={e.pacingPercentage}
                      actualAmount={e.cumulativeActual}
                      expectedYtd={e.cumulativeExpected}
                      delta={e.delta}
                      {fmt}
                    />
                  {/if}
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </TableShell>
      </DimWrapper>
    {:else if loading}
      {@render loadingSkeleton()}
    {:else if error}
      <EmptyState
        card
        title="No budget for {selectedYear}"
        description="This account has no budget for the selected year. Years without a budget stay empty."
      >
        {#snippet action()}
          <div class="flex items-center gap-2">
            <Button onclick={() => (showEditDialog = true)}>Set budget</Button>
            <Button variant="outline" onclick={() => goto(resolve('/budgets'))}
              >Back to budgets</Button
            >
          </div>
        {/snippet}
      </EmptyState>
    {:else}
      <!-- Year-mismatch window before the refetch starts; same placeholder. -->
      {@render loadingSkeleton()}
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Budget" />
    <ImportPrompt />
  </div>
{/if}

{#if accountGuid}
  {#key `${showEditDialog}-${selectedYear}`}
    <BudgetEntryDialog
      bind:open={showEditDialog}
      {accountGuid}
      year={detail?.year ?? selectedYear}
      amount={detail?.amount ?? 0}
      exists={detail !== null}
      {currency}
      onSave={handleSaveSuccess}
      onError={(msg) => (formError = msg)}
    />
  {/key}
{/if}

<Dialog.Root bind:open={showDeleteConfirm}>
  <Dialog.Content showCloseButton={false}>
    <Dialog.Header>
      <Dialog.Title>Delete Budget</Dialog.Title>
      <Dialog.Description>
        Are you sure you want to delete the {selectedYear} budget for
        <strong>{accountName}</strong>? Other years are kept. This action cannot be undone.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          showDeleteConfirm = false;
        }}>Cancel</Button
      >
      <Button variant="destructive" onclick={confirmDelete}>Delete</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
