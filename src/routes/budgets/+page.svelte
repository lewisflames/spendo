<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { cn } from '$lib/utils';
  import { dbSession } from '$lib/stores/db.svelte';
  import { sessionCurrency, formatSessionCurrency } from '$lib/utils/format.svelte';
  import { getBudgetVsActual, fetchAccounts, getBudgetDateRange } from '$lib/tauri';
  import { useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { Plus, PiggyBank, Wallet, TrendingDown, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import type { AccountNode } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import BudgetYearDialog from '$lib/features/budgets/budget-year-dialog.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import BudgetsTable from '$lib/features/budgets/budgets-table.svelte';

  const now = new Date();
  let yearSel = $state(String(now.getFullYear()));

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
    }
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () =>
      Promise.all([getBudgetVsActual(currentYear), fetchAccounts(), getBudgetDateRange()]),
    // Keep the selection within the transaction date range once the range
    // (and the URL-driven selection) is known.
    onSuccess: () => clampSelection(),
    label: 'Failed to load budgets'
  });

  // Only accounts with a budget for the selected year are listed.
  let rows = $derived(res.data?.[0] ?? null);
  let accountsData = $derived(res.data?.[1] ?? null);
  let dateRange = $derived(res.data?.[2] ?? null);
  let error = $derived(res.error);
  let formError = $state<string | null>(null);
  // Dim during refetches only; the initial load shows the skeleton instead.
  let yearLoading = $derived(res.loading && res.data !== null);

  let totalEffective = $derived((rows ?? []).reduce((s, r) => s + r.effectiveTotal, 0));
  let totalSpent = $derived((rows ?? []).reduce((s, r) => s + r.actualAmount, 0));
  let totalRemaining = $derived((rows ?? []).reduce((s, r) => s + r.remaining, 0));

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

  let minYear = $derived(
    dateRange?.earliestDate ? Number(dateRange.earliestDate.slice(0, 4)) : null
  );
  let maxYear = $derived(dateRange?.latestDate ? Number(dateRange.latestDate.slice(0, 4)) : null);

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

  function clampYear(y: number): number {
    if (minYear !== null && y < minYear) return minYear;
    if (maxYear !== null && y > maxYear) return maxYear;
    return y;
  }

  function clampSelection() {
    if (minYear === null || maxYear === null) return;
    const y = clampYear(currentYear);
    if (y !== currentYear) yearSel = String(y);
  }

  let atMin = $derived(minYear !== null && currentYear <= minYear);
  let atMax = $derived(maxYear !== null && currentYear >= maxYear);

  function handleSaveSuccess(accountGuid: string, createdYear: number) {
    res.reload();
    formError = null;
    toast.success('Budget saved');
    // Query-only year suffix on top of a resolved route.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    goto(resolve('/budgets/[accountGuid]', { accountGuid }) + `?year=${createdYear}`);
  }

  function shiftYear(delta: number) {
    yearSel = String(clampYear(currentYear + delta));
  }

  function prevYear() {
    shiftYear(-1);
  }

  function nextYear() {
    shiftYear(1);
  }

  let currency = $derived(sessionCurrency());
  const fmt = formatSessionCurrency;
</script>

<svelte:head>
  <title>Budgets — Spendo</title>
  <meta name="description" content="Yearly spending targets." />
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
    <PageHeader title="Budgets" description="Yearly spending targets · carryover always applied">
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex items-center gap-2">
          <ButtonGroup.Root>
            <Button
              variant="outline"
              size="icon"
              onclick={prevYear}
              aria-label="Previous year"
              disabled={atMin}
            >
              <ChevronLeft />
            </Button>
            {@render yearPicker()}
            <Button
              variant="outline"
              size="icon"
              onclick={nextYear}
              aria-label="Next year"
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
    {#if rows === null}
      <KpiCardRow />
    {:else}
      <DimWrapper active={yearLoading} class="grid grid-cols-1 gap-6 sm:grid-cols-3">
        <KpiCard title="Effective budget" value={fmt(totalEffective)} valueClass="text-chart-1">
          {#snippet icon()}
            <Wallet aria-hidden="true" class="size-4 text-chart-1" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Spent"
          value={fmt(totalSpent)}
          valueClass={cn(totalSpent > totalEffective ? 'text-destructive' : 'text-success')}
        >
          {#snippet icon()}
            <TrendingDown
              aria-hidden="true"
              class={cn(
                'size-4',
                totalSpent > totalEffective ? 'text-destructive' : 'text-success'
              )}
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

    {#if rows !== null && rows.length > 0}
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

    <DimWrapper active={yearLoading}>
      <BudgetsTable
        rows={rows !== null ? filteredRows : null}
        {hasBudgets}
        {currency}
        year={currentYear}
        onAddClick={() => {
          showAddDialog = true;
        }}
      />
    </DimWrapper>
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Budgets" description="Yearly spending targets" />
    <ImportPrompt />
  </div>
{/if}

<BudgetYearDialog
  bind:open={showAddDialog}
  {accountsData}
  {currency}
  {dateRange}
  initialYear={currentYear}
  onSave={handleSaveSuccess}
  onError={(msg) => (formError = msg)}
/>
