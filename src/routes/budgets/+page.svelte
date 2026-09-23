<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { SearchInput } from '$lib/components/shared/search-input';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import {
    sessionCurrency,
    formatSessionCurrency,
    clampZero,
    signColor,
    plural
  } from '$lib/utils/format.svelte';
  import { fetchBudgetVsActual, fetchAccounts, fetchBudgetDateRange } from '$lib/tauri';
  import { openBudget } from '$lib/utils/navigation';
  import { useUrlParams, yearParam, debouncedSearchParam } from '$lib/hooks/use-url-params.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { Plus, PiggyBank, Wallet, TrendingDown } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BudgetYearDialog from '$lib/features/budgets/budget-year-dialog.svelte';
  import BudgetYearStepper from '$lib/features/budgets/budget-year-stepper.svelte';
  import { accountPathMap } from '$lib/features/budgets/budget-utils';
  import { useBudgetYear } from '$lib/hooks/use-budget-year.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import BudgetsTable, {
    BUDGET_COLUMNS,
    BUDGET_DEFAULT_COLUMNS
  } from '$lib/features/budgets/budgets-table.svelte';

  const budgetCols = useColumnVisibility(BUDGET_DEFAULT_COLUMNS, 'cols:budgets');

  let showAddDialog = $state(false);

  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);

  // Instantiated before the URL restore below (which reads the selection);
  // the range getter only runs once data loads.
  const by = useBudgetYear(() => dateRange);

  // URL params are applied before the fetch is set up so the first load
  // already uses values from the URL.
  useUrlParams([
    yearParam(
      'year',
      () => by.yearSel,
      (v) => (by.yearSel = v)
    ),
    debouncedSearchParam(
      'q',
      () => search.value || null,
      (v) => {
        searchInput = v;
        search.set(v);
      }
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: async () => {
      const [budgetRows, accounts, range] = await Promise.all([
        fetchBudgetVsActual(by.year),
        fetchAccounts(),
        fetchBudgetDateRange()
      ]);
      return { budgetRows, accounts, range };
    },
    // Keep the selection within the transaction date range once the range
    // (and the URL-driven selection) is known.
    onSuccess: () => by.clampSelection(),
    label: "We couldn't load your budgets"
  });

  // Only accounts with a budget for the selected year are listed.
  let rows = $derived(res.data?.budgetRows ?? null);
  let accountsData = $derived(res.data?.accounts ?? null);
  let dateRange = $derived(res.data?.range ?? null);

  let formError = $state<string | null>(null);
  // Dim during refetches only; the initial load shows the skeleton instead.
  let yearLoading = $derived(res.loading && res.data !== null);

  let totalEffective = $derived((rows ?? []).reduce((s, r) => s + r.effectiveTotal, 0));
  let totalSpent = $derived((rows ?? []).reduce((s, r) => s + r.actualAmount, 0));
  let totalRemaining = $derived((rows ?? []).reduce((s, r) => s + r.remaining, 0));

  let accountPaths = $derived(accountPathMap(accountsData?.trees));

  let filteredRows = $derived.by(() => {
    if (!rows) return [];
    return rows
      .filter((r) => {
        if (!search.value) return true;
        const path = accountPaths[r.accountGuid]?.join(' ') ?? r.accountName;
        return path.toLowerCase().includes(search.value.toLowerCase());
      })
      .sort((a, b) => a.accountName.localeCompare(b.accountName));
  });

  let hasBudgets = $derived(rows !== null && rows.length > 0);

  function handleSaveSuccess(accountGuid: string, createdYear: number) {
    res.reload();
    formError = null;
    toast.success('Saved. Nice planning');
    openBudget(accountGuid, createdYear);
  }

  let currency = $derived(sessionCurrency());
  const fmt = formatSessionCurrency;
</script>

<svelte:head>
  <title>Budgets · Spendo</title>
  <meta name="description" content="What you plan to spend this year." />
</svelte:head>

<BookGate title="Budgets" description="What you plan to spend. Leftovers roll over">
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Budgets"
      description={rows !== null
        ? `What you plan to spend · ${plural(rows.length, 'budget')}`
        : 'What you plan to spend'}
    >
      <BudgetYearStepper
        bind:yearSel={by.yearSel}
        yearOptions={by.yearOptions}
        atMin={by.atMin}
        atMax={by.atMax}
        onPrev={() => by.shiftYear(-1)}
        onNext={() => by.shiftYear(1)}
      />
      {#if dbSession.value}
        <Button
          onclick={() => {
            showAddDialog = true;
          }}
        >
          <Plus aria-hidden="true" class="size-4" />
          New budget
        </Button>
      {/if}
    </PageHeader>

    {#if formError}
      <ErrorCard message={formError} />
    {/if}
    {#if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}

    <!-- KPI summary cards -->
    {#if rows === null && !res.error}
      <PageSkeleton variant="budgets" />
    {:else if rows !== null}
      <DimWrapper active={yearLoading} class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-3">
        <KpiCard title="Effective budget" value={fmt(clampZero(totalEffective))}>
          {#snippet icon()}
            <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Spent" value={fmt(clampZero(totalSpent))}>
          {#snippet icon()}
            <TrendingDown aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Remaining"
          value={fmt(clampZero(totalRemaining))}
          valueClass={signColor(clampZero(totalRemaining))}
        >
          {#snippet icon()}
            <PiggyBank aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
      </DimWrapper>
    {/if}

    {#if rows !== null && rows.length > 0}
      <FilterBar>
        {#snippet extra()}
          <ColumnsMenu columns={BUDGET_COLUMNS} cols={budgetCols} />
        {/snippet}
        <SearchInput
          id="budgets-search"
          label="Search budgets"
          placeholder="Search budgets…"
          bind:value={searchInput}
          class="grow basis-48"
        />
        {#if search.value}
          <FilterClearButton
            onclick={() => {
              searchInput = '';
              search.set('');
            }}
          />
        {/if}
      </FilterBar>
    {/if}

    {#if rows !== null || !res.error}
      <DimWrapper active={yearLoading}>
        <BudgetsTable
          rows={rows !== null ? filteredRows : null}
          {hasBudgets}
          {currency}
          year={by.year}
          cols={budgetCols}
          onAddClick={() => {
            showAddDialog = true;
          }}
          onClearSearch={() => {
            searchInput = '';
            search.set('');
          }}
        />
      </DimWrapper>
    {/if}
  </div>
</BookGate>

{#key showAddDialog}
  <BudgetYearDialog
    bind:open={showAddDialog}
    {accountsData}
    {currency}
    {dateRange}
    initialYear={by.year}
    onSave={handleSaveSuccess}
    onError={(msg) => (formError = msg)}
  />
{/key}
