<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { cn } from '$lib/utils';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { sessionCurrency, formatSessionCurrency } from '$lib/utils/format.svelte';
  import { listBudgets, getBudgetVsActual, fetchAccounts, getBudgetDateRange } from '$lib/tauri';
  import { useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { Plus, PiggyBank, Wallet, TrendingDown, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import type { BudgetActualRow, AccountNode } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import BudgetFormDialog from '$lib/features/budgets/budget-form-dialog.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import BudgetsTable from '$lib/features/budgets/budgets-table.svelte';

  const now = new Date();
  let monthSel = $state(String(now.getMonth() + 1));
  let yearSel = $state(String(now.getFullYear()));

  let currentMonth = $derived(Number(monthSel));
  let currentYear = $derived(Number(yearSel));

  let showAddDialog = $state(false);

  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);

  // URL params are applied before the fetch is set up so the first load
  // already uses values from the URL.
  useUrlParams([
    {
      key: 'year',
      get: () => yearSel,
      apply: (v) => {
        const y = Number(v);
        if (Number.isInteger(y) && y >= 1900 && y <= 2200) yearSel = String(y);
      },
      replace: true
    },
    {
      key: 'month',
      get: () => monthSel,
      apply: (v) => {
        const m = Number(v);
        if (Number.isInteger(m) && m >= 1 && m <= 12) monthSel = String(m);
      },
      replace: true
    }
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () =>
      Promise.all([
        listBudgets(),
        getBudgetVsActual(currentYear, currentMonth),
        fetchAccounts(),
        getBudgetDateRange()
      ]),
    // Keep the selection within the transaction date range once the range
    // (and the URL-driven selection) is known.
    onSuccess: () => clampSelection(),
    label: 'Failed to load budgets'
  });

  let budgets = $derived(res.data?.[0] ?? null);
  let budgetActuals = $derived.by(() => {
    const map: Record<string, BudgetActualRow | undefined> = {};
    for (const row of res.data?.[1] ?? []) map[row.accountGuid] = row;
    return map;
  });
  let accountsData = $derived(res.data?.[2] ?? null);
  let dateRange = $derived(res.data?.[3] ?? null);
  let error = $derived(res.error);
  let formError = $state<string | null>(null);
  // Dim during refetches only; the initial load shows the skeleton instead.
  let monthLoading = $derived(res.loading && res.data !== null);

  let totalBudgeted = $derived(
    Object.values(budgetActuals).reduce((s, r) => s + (r?.budgetAmount ?? 0), 0)
  );
  let totalSpent = $derived(
    Object.values(budgetActuals).reduce((s, r) => s + (r?.actualAmount ?? 0), 0)
  );
  let totalRemaining = $derived(
    Object.values(budgetActuals).reduce((s, r) => s + (r?.remaining ?? 0), 0)
  );

  let accountPaths = $derived.by(() => {
    const map: Record<string, string[]> = {};
    if (!accountsData?.trees) return map;
    function walk(nodes: AccountNode[], ancestors: string[]) {
      for (const n of nodes) {
        const display = n.description ?? n.name;
        const path = [...ancestors, display];
        map[n.guid] = path;
        walk(n.children, path);
      }
    }
    walk(accountsData.trees, []);
    return map;
  });

  let filteredRows = $derived.by(() => {
    if (!budgets) return [];
    return budgets
      .map((b) => ({
        budget: b,
        actual: budgetActuals[b.accountGuid]
      }))
      .filter((r) => {
        if (!search.value) return true;
        const path = accountPaths[r.budget.accountGuid]?.join(' ') ?? r.budget.accountName;
        return path.toLowerCase().includes(search.value.toLowerCase());
      })
      .sort((a, b) => a.budget.accountName.localeCompare(b.budget.accountName));
  });

  // Month-index arithmetic in one place: idx = y*12 + (m-1), invertible.
  const monthIdx = (year: number, month: number) => year * 12 + (month - 1);
  const idxYear = (idx: number) => Math.floor(idx / 12);
  const idxMonth = (idx: number) => (idx % 12) + 1;

  let minMonthIdx = $derived.by(() => {
    if (!dateRange?.earliestDate) return null;
    const [y, m] = dateRange.earliestDate.split('-').map(Number);
    return monthIdx(y, m);
  });

  let maxMonthIdx = $derived.by(() => {
    if (!dateRange?.latestDate) return null;
    const [y, m] = dateRange.latestDate.split('-').map(Number);
    return monthIdx(y, m);
  });

  let minYear = $derived(minMonthIdx === null ? null : idxYear(minMonthIdx));
  let maxYear = $derived(maxMonthIdx === null ? null : idxYear(maxMonthIdx));

  let monthOptions = $derived(
    Array.from({ length: 12 }, (_, i) => ({
      value: String(i + 1),
      label: new Date(2000, i, 1).toLocaleDateString(appSettings.value.locale, { month: 'long' })
    })).filter((o) => {
      if (minMonthIdx === null || maxMonthIdx === null) return true;
      const idx = monthIdx(currentYear, Number(o.value));
      return idx >= minMonthIdx && idx <= maxMonthIdx;
    })
  );

  let yearOptions = $derived.by(() => {
    if (minYear === null || maxYear === null) {
      const thisYear = new Date().getFullYear();
      return Array.from({ length: 26 }, (_, i) => {
        const y = thisYear - 20 + i;
        return { value: String(y), label: String(y) };
      });
    }
    return Array.from({ length: maxYear - minYear + 1 }, (_, i) => {
      const y = minYear + i;
      return { value: String(y), label: String(y) };
    });
  });

  function clampMonthIdx(idx: number): number {
    if (minMonthIdx !== null && idx < minMonthIdx) return minMonthIdx;
    if (maxMonthIdx !== null && idx > maxMonthIdx) return maxMonthIdx;
    return idx;
  }

  function clampSelection() {
    if (minMonthIdx === null || maxMonthIdx === null) return;
    const idx = clampMonthIdx(monthIdx(currentYear, currentMonth));
    const y = idxYear(idx);
    const m = idxMonth(idx);
    if (y !== currentYear || m !== currentMonth) {
      monthSel = String(m);
      yearSel = String(y);
    }
  }

  let atMin = $derived(minMonthIdx !== null && monthIdx(currentYear, currentMonth) <= minMonthIdx);
  let atMax = $derived(maxMonthIdx !== null && monthIdx(currentYear, currentMonth) >= maxMonthIdx);

  function handleSaveSuccess(accountGuid: string) {
    res.reload();
    toast.success('Budget saved');
    goto(resolve('/budgets/[accountGuid]', { accountGuid }));
  }

  function shiftMonth(delta: number) {
    const idx = clampMonthIdx(monthIdx(currentYear, currentMonth) + delta);
    monthSel = String(idxMonth(idx));
    yearSel = String(idxYear(idx));
  }

  function prevMonth() {
    shiftMonth(-1);
  }

  function nextMonth() {
    shiftMonth(1);
  }

  let currency = $derived(sessionCurrency());
  const fmt = formatSessionCurrency;
</script>

<svelte:head>
  <title>Budgets — Spendo</title>
  <meta name="description" content="Monthly spending targets." />
</svelte:head>

{#snippet yearPicker()}
  <Select.Root type="single" bind:value={yearSel} items={yearOptions}>
    <Select.Trigger class="min-w-20 justify-center" id="year-picker">
      <SelectPrimitive.Value />
    </Select.Trigger>
    <Select.Content>
      {#each yearOptions as o (o.value)}
        <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
      {/each}
    </Select.Content>
  </Select.Root>
{/snippet}

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader title="Budgets" description="Monthly spending targets">
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex items-center gap-2">
          <ButtonGroup.Root>
            <Button
              variant="outline"
              size="icon"
              onclick={prevMonth}
              aria-label="Previous month"
              disabled={atMin}
            >
              <ChevronLeft />
            </Button>
            <Select.Root type="single" bind:value={monthSel} items={monthOptions}>
              <Select.Trigger class="min-w-28 justify-center capitalize" id="month-picker">
                <SelectPrimitive.Value />
              </Select.Trigger>
              <Select.Content>
                {#each monthOptions as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            {@render yearPicker()}
            <Button
              variant="outline"
              size="icon"
              onclick={nextMonth}
              aria-label="Next month"
              disabled={atMax}
            >
              <ChevronRight />
            </Button>
          </ButtonGroup.Root>
        </div>
      </div>
    </PageHeader>

    {#if formError}
      <ErrorCard message={formError} />
    {/if}
    {#if error}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {/if}

    <!-- KPI summary cards -->
    {#if budgets === null}
      <KpiCardRow />
    {:else}
      <DimWrapper active={monthLoading} class="grid grid-cols-1 gap-6 sm:grid-cols-3">
        <KpiCard title="Budgeted" value={fmt(totalBudgeted)} valueClass="text-chart-1">
          {#snippet icon()}
            <Wallet aria-hidden="true" class="size-4 text-chart-1" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Spent"
          value={fmt(totalSpent)}
          valueClass={cn(totalSpent > totalBudgeted ? 'text-destructive' : 'text-success')}
        >
          {#snippet icon()}
            <TrendingDown
              aria-hidden="true"
              class={cn('size-4', totalSpent > totalBudgeted ? 'text-destructive' : 'text-success')}
            />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Remaining"
          value={fmt(totalRemaining)}
          valueClass={cn(totalRemaining < 0 ? 'text-destructive' : 'text-success')}
        >
          {#snippet icon()}
            <PiggyBank
              aria-hidden="true"
              class={cn('size-4', totalRemaining < 0 ? 'text-destructive' : 'text-success')}
            />
          {/snippet}
        </KpiCard>
      </DimWrapper>
    {/if}

    {#if budgets !== null && budgets.length > 0}
      <div class="flex flex-wrap items-center justify-between gap-2">
        <SearchInput
          id="budgets-search"
          label="Search budgets"
          placeholder="Search budgets…"
          bind:value={searchInput}
          class="w-full max-w-xs"
        />

        <div class="flex items-center gap-2">
          <Button
            onclick={() => {
              showAddDialog = true;
            }}
          >
            <Plus aria-hidden="true" />
            Add Budget
          </Button>
        </div>
      </div>
    {/if}

    <DimWrapper active={monthLoading}>
      <BudgetsTable
        rows={budgets !== null ? filteredRows : null}
        hasBudgets={budgets !== null && budgets.length > 0}
        {currency}
        onAddClick={() => {
          showAddDialog = true;
        }}
      />
    </DimWrapper>
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Budgets" description="Monthly spending targets" />
    <ImportPrompt />
  </div>
{/if}

<BudgetFormDialog
  bind:open={showAddDialog}
  {accountsData}
  budgetedAccountGuids={budgets?.map((b) => b.accountGuid) ?? []}
  {currency}
  onSave={handleSaveSuccess}
  onError={(msg) => (formError = msg)}
/>
