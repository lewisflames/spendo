<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Progress } from '$lib/components/ui/progress';
  import * as Table from '$lib/components/ui/table';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { cn } from '$lib/utils';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { sessionCurrency, formatSessionCurrency } from '$lib/utils/format.svelte';
  import { getBudgetDetail, getBudgetDateRange, deleteBudget } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import { useUrlParams, pageParam } from '$lib/hooks/use-url-params.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import { Pencil, Plus, Trash2, StickyNote } from '@lucide/svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import BudgetEntryDialog from '$lib/features/budgets/budget-entry-dialog.svelte';
  import BudgetOverridesDialog from '$lib/features/budgets/budget-overrides-dialog.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import type { BudgetEntry } from '$lib/tauri';

  let accountGuid = $derived(page.params.accountGuid);

  let pageSize = $derived(appSettings.value.pageSize);
  let pg = $state(1);
  const sort = useTableSort<'period' | 'amount' | 'spent' | 'remaining'>(
    { sortBy: 'period', sortDir: 'desc' },
    ['amount', 'spent', 'remaining']
  );
  let sortBy = $derived(sort.sortBy);
  let sortDir = $derived(sort.sortDir);

  // URL-param restore must be set up before useAsyncData so the first fetch
  // already uses the page from the URL.
  useUrlParams([
    pageParam(
      'page',
      () => pg,
      (p) => (pg = p)
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!accountGuid,
    fetch: () => {
      const g = accountGuid;
      if (!g) return Promise.reject(new Error('No account selected'));
      return Promise.all([getBudgetDetail(g, pg, pageSize, sortBy, sortDir), getBudgetDateRange()]);
    },
    label: 'Failed to load budget',
    onSuccess: (result) => {
      pg = result[0].page;
    }
  });

  let detail = $derived(res.data?.[0] ?? null);
  let dateRange = $derived(res.data?.[1] ?? null);
  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let formError = $state<string | null>(null);

  let showEditDialog = $state(false);
  let editEntry = $state<BudgetEntry | null>(null);
  let showDeleteConfirm = $state(false);
  let showOverridesDialog = $state(false);

  let budget = $derived(detail?.budget ?? null);
  let entries = $derived(detail?.entries ?? []);
  let currency = $derived(sessionCurrency());
  const fmt = formatSessionCurrency;

  $effect(() => {
    setBreadcrumb([
      { name: 'Budgets', href: resolve('/budgets') },
      { name: budget?.accountDescription ?? budget?.accountName ?? 'Budget' }
    ]);
  });

  let accountName = $derived(budget?.accountDescription ?? budget?.accountName ?? 'Budget');

  let prevNavKey = '';
  let prevSortKey = '';

  // Reset the page on account switch and on sort change. The reset lands after
  // the fetch effect in the same flush, so a doomed request may fire with the
  // stale page — useAsyncData cancels it and the refetch wins.
  $effect(() => {
    const g = accountGuid ?? '';
    if (prevNavKey && g !== prevNavKey) {
      pg = 1;
    }
    prevNavKey = g;

    const sk = `${sortBy}|${sortDir}`;
    if (prevSortKey && prevSortKey !== sk && pg !== 1) {
      pg = 1;
    }
    prevSortKey = sk;
  });

  function handlePageSizeChange(size: number) {
    saveSettings({ pageSize: size });
    pg = 1;
  }

  function periodLabel(e: BudgetEntry): string {
    return new Date(e.year, e.month - 1, 1).toLocaleDateString(appSettings.value.locale, {
      month: 'long',
      year: 'numeric'
    });
  }

  function remainingColor(remaining: number): string {
    return remaining < 0 ? 'text-destructive' : 'text-success';
  }

  function handleSaveSuccess(message = 'Budget amount saved') {
    res.reload();
    toast.success(message);
  }

  async function confirmDelete() {
    if (!budget) return;
    try {
      await deleteBudget(budget.guid);
      showDeleteConfirm = false;
      toast.success('Budget deleted');
      goto(resolve('/budgets'));
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
    {#if budget?.notes}
      <StickyNote aria-hidden="true" class="inline size-3.5 shrink-0 text-muted-foreground" />
    {/if}
    {budget?.notes ?? 'Budget for this account'}
  </span>
{/snippet}

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader title={accountName} description={headerDescription}>
      <div class="flex flex-wrap items-center gap-2">
        <Button variant="outline" onclick={() => (showOverridesDialog = true)} disabled={!budget}>
          <Plus aria-hidden="true" />
          Add overrides
        </Button>
        <Button variant="destructive" onclick={() => (showDeleteConfirm = true)} disabled={!budget}>
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
    {#if error && detail === null}
      <EmptyState card title="No budget for this account" description={error}>
        {#snippet action()}
          <Button variant="outline" onclick={() => goto(resolve('/budgets'))}
            >Back to budgets</Button
          >
        {/snippet}
      </EmptyState>
    {:else if detail === null}
      <TableSkeleton cols={5} rows={6} />
    {:else}
      <DimWrapper active={loading} class="flex flex-col gap-4">
        <TableShell>
          <Table.Header>
            <Table.Row class="bg-muted/75">
              <SortHead
                label="Period"
                active={sort.sortBy === 'period'}
                dir={sort.sortDir}
                onclick={() => sort.toggle('period')}
              />
              <SortHead
                label="Budget"
                active={sort.sortBy === 'amount'}
                dir={sort.sortDir}
                onclick={() => sort.toggle('amount')}
                class="text-right"
              />
              <SortHead
                label="Spent"
                active={sort.sortBy === 'spent'}
                dir={sort.sortDir}
                onclick={() => sort.toggle('spent')}
                class="text-right"
              />
              <Table.Head class="text-muted-foreground">Progress</Table.Head>
              <SortHead
                label="Remaining"
                active={sort.sortBy === 'remaining'}
                dir={sort.sortDir}
                onclick={() => sort.toggle('remaining')}
                class="text-right"
              />
              <Table.Head class="w-12"><span class="sr-only">Actions</span></Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each entries as e (e.year + '-' + e.month)}
              <Table.Row>
                <Table.Cell class="py-2.5 capitalize">
                  {periodLabel(e)}
                </Table.Cell>
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {fmt(e.amount)}
                </Table.Cell>
                <Table.Cell class="py-2.5 text-right tabular-nums">
                  {fmt(e.actualAmount)}
                </Table.Cell>
                <Table.Cell class="py-2.5">
                  <div class="flex items-center gap-2">
                    <Progress
                      value={Math.min(100, Math.max(0, e.percentage))}
                      max={100}
                      class={cn(
                        'h-1.5 w-20',
                        e.percentage > 100 &&
                          'bg-destructive/20 **:data-[slot=progress-indicator]:bg-destructive'
                      )}
                    />
                    <span
                      class={cn(
                        'text-xs tabular-nums',
                        e.percentage > 100
                          ? 'font-medium text-destructive'
                          : 'text-muted-foreground'
                      )}>{Math.max(0, e.percentage).toFixed(1)}%</span
                    >
                  </div>
                </Table.Cell>
                <Table.Cell
                  class={cn('py-2.5 text-right tabular-nums', remainingColor(e.remaining))}
                >
                  {e.remaining >= 0 ? `+${fmt(e.remaining)}` : fmt(e.remaining)}
                </Table.Cell>
                <Table.Cell class="py-2.5">
                  <div class="flex items-center justify-end">
                    <Tooltip.Root>
                      <Tooltip.Trigger>
                        {#snippet child({ props })}
                          <Button
                            {...props}
                            variant="ghost"
                            size="icon-sm"
                            onclick={() => {
                              editEntry = e;
                              showEditDialog = true;
                            }}
                            aria-label="Edit budget amount"
                          >
                            <Pencil class="size-3" />
                          </Button>
                        {/snippet}
                      </Tooltip.Trigger>
                      <Tooltip.Content>
                        <p>Edit amount</p>
                      </Tooltip.Content>
                    </Tooltip.Root>
                  </div>
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </TableShell>

        <PaginationNav
          bind:page={pg}
          totalPages={detail.totalPages}
          totalItems={detail.total}
          pageSize={appSettings.value.pageSize}
          onPageSizeChange={handlePageSizeChange}
        />
      </DimWrapper>
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Budget" />
    <ImportPrompt />
  </div>
{/if}

{#if editEntry}
  <BudgetEntryDialog
    bind:open={showEditDialog}
    accountGuid={accountGuid!}
    entry={editEntry}
    {currency}
    onSave={handleSaveSuccess}
    onError={(msg) => (formError = msg)}
  />
{/if}

{#if budget}
  <BudgetOverridesDialog
    bind:open={showOverridesDialog}
    accountGuid={accountGuid!}
    {currency}
    {dateRange}
    onSave={() => handleSaveSuccess('Budget overrides saved')}
    onError={(msg) => (formError = msg)}
  />
{/if}

<Dialog.Root bind:open={showDeleteConfirm}>
  <Dialog.Content showCloseButton={false}>
    <Dialog.Header>
      <Dialog.Title>Delete Budget</Dialog.Title>
      <Dialog.Description>
        Are you sure you want to delete the budget for <strong>{accountName}</strong>? All budget
        amounts for this account will be removed. This action cannot be undone.
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
