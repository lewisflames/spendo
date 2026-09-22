<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { SvelteSet } from 'svelte/reactivity';
  import { ArrowLeft, CalendarDays, CircleCheck, SearchX, X } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import { dbSession, refreshSession } from '$lib/stores/db.svelte';
  import {
    fetchReconcileInfo,
    finishReconciliation,
    markReconcileSplits,
    type TxSortKey
  } from '$lib/tauri';
  import { sessionCurrency, plural } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { today } from '$lib/utils/date-utils.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useUrlParams, dateParam, debouncedSearchParam } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import ReconcileStatementForm from '$lib/features/reconcile/reconcile-statement-form.svelte';
  import ReconcileTable from '$lib/features/reconcile/reconcile-table.svelte';
  import {
    clearedTotal,
    isBalanced,
    parseStatementAmount,
    statementDifference,
    targetBalance
  } from '$lib/features/reconcile/math';

  let guid = $derived(page.params.guid);
  let currency = $derived(sessionCurrency());

  let statementDate = $state(today.value);
  let endingBalanceInput = $state('');
  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);
  // Oldest-first is deliberate for reconcile (check off in statement order),
  // opposite of the ledger default (newest-first).
  const sort = useTableSort<'date' | 'description' | 'amount'>({ sortBy: 'date', sortDir: 'asc' }, [
    'amount'
  ]);

  useUrlParams([
    dateParam(
      'statement',
      () => (statementDate === today.value ? null : statementDate || null),
      (v) => (statementDate = v ?? today.value)
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
    enabled: () => !!dbSession.value && !!guid && /^\d{4}-\d{2}-\d{2}$/.test(statementDate),
    fetch: () => {
      const g = guid;
      if (!g) return Promise.reject(new Error('No account selected'));
      return fetchReconcileInfo(g, statementDate);
    },
    label: 'Failed to load reconciliation data'
  });

  let candidates = $derived(res.data?.candidates ?? []);
  let startingBalance = $derived(res.data?.startingBalance ?? 0);

  // Local cleared set, seeded from the backend 'c' states per statement.
  // SvelteSet (not a reassigned $state Set) so checkbox toggles stay reactive.
  let checked = new SvelteSet<string>();
  let syncedKey = $state('');

  $effect(() => {
    const info = res.data;
    if (!info) return;
    const key = `${info.accountGuid}|${info.statementDate}`;
    if (key !== syncedKey) {
      syncedKey = key;
      checked.clear();
      for (const c of info.candidates) {
        if (c.reconcileState === 'c') checked.add(c.splitGuid);
      }
    }
  });

  let originalChecked = $derived(
    new Set(candidates.filter((c) => c.reconcileState === 'c').map((c) => c.splitGuid))
  );
  let dirty = $derived.by(() => {
    if (checked.size !== originalChecked.size) return true;
    for (const g of checked) if (!originalChecked.has(g)) return true;
    return false;
  });

  let amountByGuid = $derived(new Map(candidates.map((c) => [c.splitGuid, c.amount] as const)));
  let cleared = $derived(clearedTotal([...checked].map((g) => amountByGuid.get(g) ?? 0)));
  let target = $derived(targetBalance(startingBalance, cleared));
  let endingBalance = $derived(parseStatementAmount(endingBalanceInput));
  let difference = $derived(
    endingBalance === null ? NaN : statementDifference(endingBalance, target)
  );
  let balanced = $derived(endingBalance !== null && isBalanced(difference));
  let saving = $state(false);
  let finishing = $state(false);
  // Finish auto-saves dirty marks first, so dirty must not block it — only a
  // missing/invalid ending balance, an unbalanced statement, or an empty
  // selection does.
  let canFinish = $derived(
    !!res.data && endingBalance !== null && balanced && !saving && !finishing && checked.size > 0
  );

  let filtered = $derived.by(() => {
    const q = search.value.trim().toLowerCase();
    const base = q
      ? candidates.filter((c) =>
          [c.description, c.num, c.memo].some((f) => f?.toLowerCase().includes(q))
        )
      : candidates;
    const sorted = [...base];
    const dir = sort.sortDir === 'asc' ? 1 : -1;
    sorted.sort((a, b) => {
      if (sort.sortBy === 'amount') return (a.amount - b.amount || 0) * dir;
      if (sort.sortBy === 'description')
        return (
          (a.description ?? '').localeCompare(b.description ?? '', undefined, {
            sensitivity: 'base'
          }) * dir
        );
      if (a.postDate !== b.postDate) return (a.postDate < b.postDate ? -1 : 1) * dir;
      return (a.splitGuid < b.splitGuid ? -1 : 1) * dir;
    });
    return sorted;
  });

  function toggle(splitGuid: string, next: boolean) {
    if (next) checked.add(splitGuid);
    else checked.delete(splitGuid);
  }

  function toggleAll(next: boolean) {
    for (const row of filtered) {
      if (next) checked.add(row.splitGuid);
      else checked.delete(row.splitGuid);
    }
  }

  function pendingUpdates() {
    return candidates
      .filter((c) => {
        const want = checked.has(c.splitGuid) ? 'c' : 'n';
        return want !== c.reconcileState;
      })
      .map((c) => ({
        splitGuid: c.splitGuid,
        state: checked.has(c.splitGuid) ? 'c' : 'n'
      }));
  }

  async function save() {
    const g = guid;
    if (!g) return;
    const updates = pendingUpdates();
    if (updates.length === 0) return;
    saving = true;
    try {
      await markReconcileSplits(g, updates);
      // Saving progress doesn't move the summary counters; refresh the session
      // so nav roots and badges pick up any changes.
      refreshSession();
      toast.success('Progress saved');
      res.reload();
    } catch (e) {
      toast.error(errMsg(e, 'Failed to save progress'));
    } finally {
      saving = false;
    }
  }

  async function finish() {
    const g = guid;
    if (!g || endingBalance === null) return;
    finishing = true;
    try {
      // Auto-save dirty cleared marks first so Finish is a single click: the
      // backend promotes every 'c' split at or before the statement date.
      const updates = pendingUpdates();
      if (updates.length > 0) {
        await markReconcileSplits(g, updates);
      }
      const out = await finishReconciliation(g, statementDate, endingBalance);
      toast.success(`Reconciled ${plural(out.reconciledCount, 'split')}`);
      void goto(resolve('/reconcile'));
    } catch (e) {
      toast.error(errMsg(e, 'Failed to finish reconciliation'));
    } finally {
      finishing = false;
    }
  }

  function onSort(key: TxSortKey) {
    if (key === 'date' || key === 'description' || key === 'amount') sort.toggle(key);
  }
</script>

<svelte:head>
  <title>{res.data ? `Reconcile ${res.data.accountName} — Spendo` : 'Reconcile — Spendo'}</title>
  <meta name="description" content="Reconcile an account against a bank statement." />
</svelte:head>

<BookGate title={res.data ? `Reconcile ${res.data.accountName}` : 'Reconcile'}>
  <div class="flex flex-col gap-6">
    <PageHeader title={res.data ? `Reconcile ${res.data.accountName}` : 'Reconcile'}>
      {#snippet description()}
        Check off cleared transactions until the difference is zero, then finish.
      {/snippet}
      <Button variant="outline" href={resolve('/reconcile')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Reconcile
      </Button>
    </PageHeader>

    {#if res.loading && !res.data && !res.error}
      <TableSkeleton cols={5} rows={8} />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      <DimWrapper active={res.loading} class="flex flex-col gap-6">
        <ReconcileStatementForm
          bind:statementDate
          bind:endingBalanceInput
          {startingBalance}
          {cleared}
          {target}
          {difference}
          {balanced}
          checkedCount={checked.size}
          totalCount={candidates.length}
          {dirty}
          {saving}
          {finishing}
          {canFinish}
          onSave={save}
          onFinish={finish}
        />

        <div class="flex flex-wrap items-end gap-2">
          <SearchInput
            id="reconcile-search"
            label="Search description, number, memo"
            placeholder="Search description, number, memo…"
            bind:value={searchInput}
            class="grow basis-48"
          />
          {#if search.value}
            <Button
              variant="ghost"
              onclick={() => {
                searchInput = '';
                search.set('');
              }}
            >
              <X aria-hidden="true" class="mr-1 size-3.5" />
              Clear
            </Button>
          {/if}
        </div>

        {#if filtered.length === 0}
          <EmptyState
            card
            title="Nothing to reconcile"
            description={search.value
              ? 'No candidates match the current filters.'
              : 'No unreconciled transactions at or before the statement date.'}
          >
            {#snippet icon()}
              {#if search.value}
                <SearchX aria-hidden="true" class="text-muted-foreground" />
              {:else}
                <CircleCheck aria-hidden="true" class="text-muted-foreground" />
              {/if}
            {/snippet}
            {#snippet action()}
              {#if search.value}
                <Button
                  variant="outline"
                  onclick={() => {
                    searchInput = '';
                    search.set('');
                  }}
                >
                  Clear filters
                </Button>
              {/if}
            {/snippet}
          </EmptyState>
        {:else}
          <ReconcileTable
            rows={filtered}
            {checked}
            {currency}
            sortBy={sort.sortBy}
            sortDir={sort.sortDir}
            {onSort}
            onToggle={toggle}
            onToggleAll={toggleAll}
          />
        {/if}
      </DimWrapper>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if res.loading}
      <TableSkeleton cols={5} rows={8} />
    {:else}
      <EmptyState
        card
        title="Choose a statement date"
        description="Enter a valid statement date to load reconciliation candidates."
      >
        {#snippet icon()}
          <CalendarDays aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
      </EmptyState>
    {/if}
  </div>
</BookGate>
