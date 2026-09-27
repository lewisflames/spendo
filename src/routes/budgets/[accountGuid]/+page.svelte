<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import { dbSession, dbBootstrapping } from '$lib/stores/db.svelte';
  import { sessionCurrency } from '$lib/utils/format.svelte';
  import { fetchBudgetDetail, fetchBudgetDateRange, fetchAccounts } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useUrlParams, yearParam } from '$lib/hooks/use-url-params.svelte';
  import { ArrowLeft, Pencil, PiggyBank, Plus } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BudgetEntryDialog from '$lib/features/budgets/budget-entry-dialog.svelte';
  import BudgetSummaryCards from '$lib/features/budgets/budget-summary-cards.svelte';
  import BudgetMonthlyTable from '$lib/features/budgets/budget-monthly-table.svelte';
  import BudgetYearStepper from '$lib/features/budgets/budget-year-stepper.svelte';
  import { accountNameMap } from '$lib/features/budgets/budget-utils';
  import { useBudgetYear } from '$lib/hooks/use-budget-year.svelte';
  import { isNotFoundMessage } from '$lib/utils/error';
  import BookGate from '$lib/components/shared/book-gate.svelte';

  let accountGuid = $derived(page.params.accountGuid);

  // Detail view extends one year beyond the latest transaction so next year’s
  // budget can be planned ahead (mirrors BudgetYearDialog).
  const by = useBudgetYear(() => dateRange, { plusOne: true });

  // URL-param restore must be set up before useAsyncData so the first fetch
  // already uses the year from the URL.
  useUrlParams([
    yearParam(
      'year',
      () => by.yearSel,
      (v) => (by.yearSel = v)
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!accountGuid,
    fetch: async () => {
      const g = accountGuid;
      if (!g) return Promise.reject(new Error('No account selected'));
      const [detail, range, accounts] = await Promise.all([
        fetchBudgetDetail(g, by.year),
        fetchBudgetDateRange(),
        fetchAccounts()
      ]);
      return { detail, range, accounts };
    },
    label: 'We couldn’t load this budget',
    onSuccess: () => by.clampSelection()
  });

  // Previous data is kept by useAsyncData while refetching: only accept the
  // detail when it matches the selected year, so switching to a year without
  // a budget falls through to the empty state instead of showing stale data.
  let detail = $derived(
    res.data?.detail && res.data.detail.budget.year === by.year ? res.data.detail : null
  );
  let dateRange = $derived(res.data?.range ?? null);
  let accountsData = $derived(res.data?.accounts ?? null);

  // A missing budget row surfaces as a backend "No budget for …" error.
  // Treat that as an empty state so a year without a target shows the
  // consistent EmptyState instead of an ErrorCard; genuine failures still
  // use ErrorCard. Uses the shared not-found helper with the other details.
  let isMissing = $derived(detail === null && isNotFoundMessage(res.error));
  let formError = $state<string | null>(null);

  let showEditDialog = $state(false);

  let budget = $derived(detail?.budget ?? null);
  let summary = $derived(detail?.summary ?? null);
  let monthlyEntries = $derived(detail?.monthlyEntries ?? []);
  let currency = $derived(sessionCurrency());

  // Account display name for years without a budget row (resolved via accounts).
  let accountNames = $derived(accountNameMap(accountsData?.trees));

  let accountName = $derived(
    budget?.accountName ?? (accountGuid ? accountNames[accountGuid] : undefined) ?? 'Budget'
  );
  let accountDescription = $derived(budget?.accountDescription ?? null);

  function handleSaveSuccess(message = 'Annual budget saved') {
    res.reload();
    formError = null;
    toast.success(message);
  }

  function handleDeleteSuccess() {
    toast.success('Budget deleted');
    // The viewed year no longer has a budget — back to the list.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(resolve('/budgets') + `?year=${by.year}`);
  }
</script>

<svelte:head>
  <title>{accountName} · Spendo</title>
  <meta name="description" content="Yearly budget for this account." />
</svelte:head>

<BookGate title="Budget" description="Yearly budget for this account">
  <div class="flex flex-col gap-6">
    <PageHeader
      title={accountName}
      description={accountDescription ?? 'Yearly budget for this account'}
    >
      <Button variant="outline" href={resolve('/budgets')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Budgets
      </Button>
      <BudgetYearStepper
        bind:yearSel={by.yearSel}
        yearOptions={by.yearOptions}
        atMin={by.atMin}
        atMax={by.atMax}
        onPrev={() => by.shiftYear(-1)}
        onNext={() => by.shiftYear(1)}
      />
      <Button onclick={() => (showEditDialog = true)} disabled={detail === null && res.loading}>
        <Pencil aria-hidden="true" class="size-4" />
        {detail ? 'Edit budget' : 'Set budget'}
      </Button>
    </PageHeader>

    {#if formError}
      <ErrorCard message={formError} />
    {/if}
    {#if res.error && detail !== null}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
    {#if detail !== null && summary !== null}
      <DimWrapper active={res.loading} class="flex flex-col gap-6">
        <BudgetSummaryCards {summary} annualTarget={detail.budget.amount} {currency} />
        <BudgetMonthlyTable
          entries={monthlyEntries}
          elapsedMonths={summary.elapsedMonths}
          {currency}
        />
      </DimWrapper>
    {:else if res.loading || dbBootstrapping.value}
      <PageSkeleton variant="budget-detail" />
    {:else if isMissing || (detail === null && !res.error)}
      <EmptyState
        card
        title="No budget for {by.year}"
        description="This account has no budget for the selected year. Years without a budget stay empty."
      >
        {#snippet icon()}
          <PiggyBank aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
        {#snippet action()}
          <Button onclick={() => (showEditDialog = true)}>
            <Plus aria-hidden="true" class="size-4" />
            Set budget
          </Button>
        {/snippet}
      </EmptyState>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else}
      <!-- Year-mismatch window before the refetch starts; same placeholder. -->
      <PageSkeleton variant="budget-detail" />
    {/if}
  </div>
</BookGate>

{#if accountGuid}
  {#key `${accountGuid}-${showEditDialog}-${by.year}`}
    <BudgetEntryDialog
      bind:open={showEditDialog}
      {accountGuid}
      year={detail?.budget.year ?? by.year}
      amount={detail?.budget.amount ?? 0}
      exists={detail !== null}
      {currency}
      {accountName}
      onSave={handleSaveSuccess}
      onDelete={handleDeleteSuccess}
    />
  {/key}
{/if}
