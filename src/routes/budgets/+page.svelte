<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { cn } from '$lib/utils';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { sessionCurrency, formatSessionCurrency } from '$lib/utils/format.svelte';
  import {
    listBudgets,
    deleteBudget,
    getBudgetVsActual,
    fetchAccounts,
    getBudgetDateRange
  } from '$lib/tauri';
  import { useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { Plus, PiggyBank, Wallet, TrendingDown, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import type { Budget, BudgetActualRow, AccountNode } from '$lib/tauri';
  import type { BudgetRow, BudgetGroup } from '$lib/features/budgets/budget-types';
  import { errMsg } from '$lib/utils/error';
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
  let viewMode = $state<'MONTHLY' | 'YEARLY'>('MONTHLY');

  let currentMonth = $derived(Number(monthSel));
  let currentYear = $derived(Number(yearSel));

  let showAddDialog = $state(false);
  let editBudget = $state<Budget | null>(null);
  let showDeleteConfirm = $state(false);
  let deleteTargetGuid = $state<string | null>(null);
  let deleteTargetName = $state('');

  let search = $state('');

  // URL params are applied before the fetch is set up so the first load
  // already uses values from the URL.
  useUrlParams([
    {
      key: 'view',
      get: () => viewMode,
      apply: (v) => {
        if (v === 'MONTHLY' || v === 'YEARLY') viewMode = v;
      },
      replace: true
    },
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
        getBudgetVsActual(currentYear, currentMonth, viewMode),
        fetchAccounts(),
        getBudgetDateRange()
      ]),
    label: 'Failed to load budgets'
  });

  let budgets = $derived(res.data?.[0] ?? null);
  let budgetActuals = $derived.by(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient snapshot
    const map = new Map<string, BudgetActualRow>();
    for (const row of res.data?.[1] ?? []) map.set(row.accountGuid, row);
    return map;
  });
  let accountsData = $derived(res.data?.[2] ?? null);
  let dateRange = $derived(res.data?.[3] ?? null);
  let error = $derived(res.error);
  // Dim during refetches only; the initial load shows the skeleton instead.
  let monthLoading = $derived(res.loading && res.data !== null);

  let totalBudgeted = $derived([...budgetActuals.values()].reduce((s, r) => s + r.budgetAmount, 0));
  let totalSpent = $derived([...budgetActuals.values()].reduce((s, r) => s + r.actualAmount, 0));
  let totalRemaining = $derived([...budgetActuals.values()].reduce((s, r) => s + r.remaining, 0));

  let accountPaths = $derived.by(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient snapshot
    const map = new Map<string, string[]>();
    if (!accountsData?.trees) return map;
    function walk(nodes: AccountNode[], ancestors: string[]) {
      for (const n of nodes) {
        const display = n.description ?? n.name;
        const path = [...ancestors, display];
        map.set(n.guid, path);
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
        actual: budgetActuals.get(b.accountGuid)
      }))
      .filter((r) => {
        if (!search) return true;
        const path = accountPaths.get(r.budget.accountGuid)?.join(' ') ?? r.budget.accountName;
        return path.toLowerCase().includes(search.toLowerCase());
      })
      .sort((a, b) => a.budget.accountName.localeCompare(b.budget.accountName));
  });

  let groups: BudgetGroup[] = $derived.by(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient snapshot
    const map = new Map<string, BudgetRow[]>();
    for (const row of filteredRows) {
      const path = accountPaths.get(row.budget.accountGuid);
      let category = 'Other';
      if (path && path.length >= 2) category = path[path.length - 2];
      else if (path && path.length === 1) category = path[0];
      else if (row.budget.accountName) category = row.budget.accountName;
      const arr = map.get(category) ?? [];
      arr.push(row);
      map.set(category, arr);
    }
    return [...map.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([category, rows]) => ({ category, rows }));
  });

  let minMonthIdx = $derived.by(() => {
    if (!dateRange?.earliestDate) return null;
    const [y, m] = dateRange.earliestDate.split('-').map(Number);
    return y * 12 + (m - 1);
  });

  let maxMonthIdx = $derived.by(() => {
    if (!dateRange?.latestDate) return null;
    const [y, m] = dateRange.latestDate.split('-').map(Number);
    return y * 12 + (m - 1);
  });

  let minYear = $derived(minMonthIdx === null ? null : Math.floor(minMonthIdx / 12));
  let maxYear = $derived(maxMonthIdx === null ? null : Math.floor(maxMonthIdx / 12));

  let monthOptions = $derived(
    Array.from({ length: 12 }, (_, i) => ({
      value: String(i + 1),
      label: new Date(2000, i, 1).toLocaleDateString(appSettings.value.locale, { month: 'long' })
    })).filter((o) => {
      if (minMonthIdx === null || maxMonthIdx === null) return true;
      const idx = currentYear * 12 + (Number(o.value) - 1);
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
    const idx = clampMonthIdx(currentYear * 12 + (currentMonth - 1));
    const y = Math.floor(idx / 12);
    const m = (idx % 12) + 1;
    if (y !== currentYear || m !== currentMonth) {
      monthSel = String(m);
      yearSel = String(y);
    }
  }

  let atMin = $derived(
    minMonthIdx !== null && currentYear * 12 + (currentMonth - 1) <= minMonthIdx
  );
  let atMax = $derived(
    maxMonthIdx !== null && currentYear * 12 + (currentMonth - 1) >= maxMonthIdx
  );

  // Keep the selection within the transaction date range
  $effect(() => {
    clampSelection();
  });

  $effect(() => {
    if (!showAddDialog) {
      editBudget = null;
    }
  });

  function handleEdit(b: Budget) {
    editBudget = b;
    showAddDialog = true;
  }

  async function handleSaveSuccess() {
    res.reload();
    toast.success('Budget saved');
  }

  function handleDeleteClick(guid: string, name: string) {
    deleteTargetGuid = guid;
    deleteTargetName = name;
    showDeleteConfirm = true;
  }

  async function confirmDelete() {
    if (!deleteTargetGuid) return;
    try {
      await deleteBudget(deleteTargetGuid);
      showDeleteConfirm = false;
      deleteTargetGuid = null;
      deleteTargetName = '';
      res.reload();
      toast.success('Budget deleted');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to delete budget'));
    }
  }

  function shiftMonth(delta: number) {
    const idx = clampMonthIdx(currentYear * 12 + (currentMonth - 1) + delta);
    monthSel = String((idx % 12) + 1);
    yearSel = String(Math.floor(idx / 12));
  }

  function prevMonth() {
    shiftMonth(-1);
  }

  function nextMonth() {
    shiftMonth(1);
  }

  function shiftYear(delta: number) {
    let y = currentYear + delta;
    if (minYear !== null && y < minYear) y = minYear;
    if (maxYear !== null && y > maxYear) y = maxYear;
    yearSel = String(y);
  }

  let currency = $derived(sessionCurrency());
  const fmt = formatSessionCurrency;
</script>

<svelte:head>
  <title>Budgets — Spendo</title>
  <meta name="description" content="Monthly and annual spending targets." />
</svelte:head>

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Budgets"
      description={viewMode === 'MONTHLY' ? 'Monthly spending targets' : 'Annual spending targets'}
    >
      <div class="flex flex-wrap items-center gap-2">
        <Button
          onclick={() => {
            editBudget = null;
            showAddDialog = true;
          }}
          disabled={!dbSession.value}
        >
          <Plus aria-hidden="true" />
          Add Budget
        </Button>

        <div class="flex items-center gap-1 rounded-md border bg-muted/40 p-0.5">
          <Button
            variant={viewMode === 'MONTHLY' ? 'default' : 'ghost'}
            size="sm"
            class="h-7"
            onclick={() => (viewMode = 'MONTHLY')}
            disabled={!dbSession.value}
          >
            Month
          </Button>
          <Button
            variant={viewMode === 'YEARLY' ? 'default' : 'ghost'}
            size="sm"
            class="h-7"
            onclick={() => (viewMode = 'YEARLY')}
            disabled={!dbSession.value}
          >
            Year
          </Button>
        </div>
      </div>
    </PageHeader>

    {#if error}
      <ErrorCard message={error} />
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

    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex items-center gap-2">
        {#if viewMode === 'MONTHLY'}
          <ButtonGroup.Root>
            <Button
              variant="outline"
              size="icon"
              onclick={prevMonth}
              aria-label="Previous month"
              disabled={!dbSession.value || atMin}
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
            <Button
              variant="outline"
              size="icon"
              onclick={nextMonth}
              aria-label="Next month"
              disabled={!dbSession.value || atMax}
            >
              <ChevronRight />
            </Button>
          </ButtonGroup.Root>
        {:else}
          <ButtonGroup.Root>
            <Button
              variant="outline"
              size="icon"
              onclick={() => shiftYear(-1)}
              aria-label="Previous year"
              disabled={!dbSession.value || (minYear !== null && currentYear <= minYear)}
            >
              <ChevronLeft />
            </Button>
            <Select.Root type="single" bind:value={yearSel} items={yearOptions}>
              <Select.Trigger class="min-w-28 justify-center" id="year-picker">
                <SelectPrimitive.Value />
              </Select.Trigger>
              <Select.Content>
                {#each yearOptions as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            <Button
              variant="outline"
              size="icon"
              onclick={() => shiftYear(1)}
              aria-label="Next year"
              disabled={!dbSession.value || (maxYear !== null && currentYear >= maxYear)}
            >
              <ChevronRight />
            </Button>
          </ButtonGroup.Root>
        {/if}
      </div>

      {#if budgets !== null && budgets.length > 0}
        <SearchInput
          id="budgets-search"
          label="Search budgets"
          placeholder="Search budgets…"
          bind:value={search}
          class="w-full max-w-xs"
        />
      {/if}
    </div>

    <DimWrapper active={monthLoading}>
      <BudgetsTable
        groups={budgets !== null ? groups : null}
        hasBudgets={budgets !== null && budgets.length > 0}
        {currency}
        {viewMode}
        onEdit={handleEdit}
        onDeleteClick={handleDeleteClick}
        onAddClick={() => {
          editBudget = null;
          showAddDialog = true;
        }}
      />
    </DimWrapper>
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Budgets"
      description={viewMode === 'MONTHLY' ? 'Monthly spending targets' : 'Annual spending targets'}
    />
    <ImportPrompt />
  </div>
{/if}

<BudgetFormDialog
  bind:open={showAddDialog}
  budget={editBudget}
  {accountsData}
  viewYear={currentYear}
  {currency}
  onSave={handleSaveSuccess}
  onError={(msg) => (error = msg)}
/>

<Dialog.Root bind:open={showDeleteConfirm}>
  <Dialog.Content showCloseButton={false}>
    <Dialog.Header>
      <Dialog.Title>Delete Budget</Dialog.Title>
      <Dialog.Description>
        Are you sure you want to delete the budget for <strong>{deleteTargetName}</strong>? This
        action cannot be undone.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          showDeleteConfirm = false;
          deleteTargetGuid = null;
        }}>Cancel</Button
      >
      <Button variant="destructive" onclick={confirmDelete}>Delete</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
