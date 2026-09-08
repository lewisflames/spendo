<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { SvelteSet } from 'svelte/reactivity';
  import { ArrowLeft, CircleCheck } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import { dbSession } from '$lib/stores/db.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import {
    fetchReconcileInfo,
    finishReconciliation,
    markReconcileSplits,
    type TxSortKey
  } from '$lib/tauri';
  import { sessionCurrency } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
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
    statementDifference,
    targetBalance
  } from '$lib/features/reconcile/math';

  let guid = $derived(page.params.guid);
  let currency = $derived(sessionCurrency());

  let statementDate = $state(todayStr());
  let endingBalanceInput = $state('');
  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);
  const sort = useTableSort<'date' | 'description' | 'amount'>({ sortBy: 'date', sortDir: 'asc' }, [
    'amount'
  ]);

  useUrlParams([
    dateParam(
      'statement',
      () => (statementDate === todayStr() ? null : statementDate || null),
      (v) => (statementDate = v ?? todayStr())
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
  let endingBalance = $derived.by(() => {
    const n = Number(endingBalanceInput);
    return endingBalanceInput.trim() === '' || !Number.isFinite(n) ? null : n;
  });
  let difference = $derived(
    endingBalance === null ? NaN : statementDifference(endingBalance, target)
  );
  let balanced = $derived(endingBalance !== null && isBalanced(difference));
  let canFinish = $derived(
    !!res.data && endingBalance !== null && balanced && !dirty && checked.size > 0
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
          ((a.description ?? '').toLowerCase() < (b.description ?? '').toLowerCase() ? -1 : 1) * dir
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

  let saving = $state(false);
  let finishing = $state(false);

  async function save() {
    const g = guid;
    if (!g) return;
    const updates = pendingUpdates();
    if (updates.length === 0) return;
    saving = true;
    try {
      await markReconcileSplits(g, updates);
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
      const out = await finishReconciliation(g, statementDate, endingBalance);
      toast.success(
        `Reconciled ${out.reconciledCount} split${out.reconciledCount === 1 ? '' : 's'}`
      );
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

  $effect(() => {
    void page.url.pathname;
    setBreadcrumb([
      { name: 'Reconcile', href: resolve('/reconcile') },
      { name: res.data?.accountName ?? 'Account' }
    ]);
  });
</script>

<svelte:head>
  <title>{res.data ? `Reconcile ${res.data.accountName} — Spendo` : 'Reconcile — Spendo'}</title>
  <meta name="description" content="Reconcile an account against a bank statement." />
</svelte:head>

<BookGate title="Reconcile">
  <div class="flex flex-col gap-6">
    <PageHeader title={res.data ? `Reconcile ${res.data.accountName}` : 'Reconcile'}>
      {#snippet description()}
        Check off cleared transactions until the difference is zero, then finish.
      {/snippet}
      <Button variant="outline" href={resolve('/reconcile')}>
        <ArrowLeft aria-hidden="true" class="size-3" />
        All accounts
      </Button>
    </PageHeader>

    {#if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else}
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

      <div class="max-w-md">
        <SearchInput
          id="reconcile-search"
          placeholder="Filter by description, number, memo…"
          bind:value={searchInput}
        />
      </div>

      {#if res.loading && !res.data}
        <TableSkeleton cols={5} rows={8} />
      {:else if filtered.length === 0}
        <EmptyState
          card
          title="Nothing to reconcile"
          description={search.value
            ? 'No candidates match the current filter.'
            : 'No unreconciled transactions at or before the statement date.'}
        >
          {#snippet icon()}
            <CircleCheck aria-hidden="true" class="text-muted-foreground" />
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
        />
      {/if}
    {/if}
  </div>
</BookGate>
