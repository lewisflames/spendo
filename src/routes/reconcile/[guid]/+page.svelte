<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import OverflowMenu from '$lib/components/shared/overflow-menu.svelte';
  import { page } from '$app/state';
  import { beforeNavigate, goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { isTauri } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { SvelteSet } from 'svelte/reactivity';
  import { CalendarDays, CircleCheck, Undo2, X } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import { dbSession, dbBootstrapping, refreshSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import {
    fetchReconcileInfo,
    finishReconciliation,
    markReconcileSplits,
    reopenLastReconciliation,
    type TxSortKey
  } from '$lib/tauri';
  import { sessionCurrency, plural, formatSignedCurrency } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { today } from '$lib/utils/date-utils.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import FilteredEmptyState from '$lib/components/shared/filtered-empty-state.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import { SearchInput } from '$lib/components/shared/search-input';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useUrlParams, dateParam, debouncedSearchParam } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useTableSort } from '$lib/hooks/use-table-sort.svelte';
  import ReconcileKpiCards from '$lib/features/reconcile/reconcile-kpi-cards.svelte';
  import ReconcileStatementForm from '$lib/features/reconcile/reconcile-statement-form.svelte';
  import ReconcileTable from '$lib/features/reconcile/reconcile-table.svelte';
  import ReconcileLeaveDialog from '$lib/features/reconcile/reconcile-leave-dialog.svelte';
  import ReconcileFinishDialog from '$lib/features/reconcile/reconcile-finish-dialog.svelte';
  import ReconcileUndoDialog from '$lib/features/reconcile/reconcile-undo-dialog.svelte';
  import {
    filterAndSortCandidates,
    pendingReconcileUpdates
  } from '$lib/features/reconcile/candidates';
  import {
    readReconcileDraft,
    removeReconcileDraft,
    writeReconcileDraft
  } from '$lib/features/reconcile/reconcile-draft';
  import {
    clearedTotal,
    duplicateCandidateGuids,
    isBalanced,
    parseStatementAmount,
    rangeSplitGuids,
    statementDifference,
    targetBalance
  } from '$lib/features/reconcile/math';

  let guid = $derived(page.params.guid);
  let session = $derived(sessionCurrency());

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
    label: 'We couldn’t load this reconciliation'
  });

  let candidates = $derived(res.data?.candidates ?? []);
  let startingBalance = $derived(res.data?.startingBalance ?? 0);
  // Statement amounts are in the account's commodity (what the bank shows);
  // only the account list converts balances to the report currency.
  let currency = $derived(res.data?.commodity ?? session);

  // Local cleared set, seeded from the backend 'c' states per statement.
  // SvelteSet (not a reassigned $state Set) so checkbox toggles stay reactive.
  let checked = new SvelteSet<string>();
  // Identity of the last data object we seeded from: a reload (save, undo)
  // yields a fresh object, which reseeds; same-object renders only prune.
  let syncedInfo = $state<object | null>(null);
  let lastClicked = $state<string | null>(null);

  $effect(() => {
    const info = res.data;
    if (!info) return;
    if (info !== syncedInfo) {
      syncedInfo = info;
      checked.clear();
      for (const c of info.candidates) {
        if (c.reconcileState === 'c') checked.add(c.splitGuid);
      }
      lastClicked = null;
      return;
    }
    // Same data reloaded in place: drop guids that disappeared (e.g.
    // reconciled elsewhere) so `dirty` can't wedge on rows no longer shown.
    const known = new Set(info.candidates.map((c) => c.splitGuid));
    for (const g of [...checked]) {
      if (!known.has(g)) checked.delete(g);
    }
    if (lastClicked && !known.has(lastClicked)) lastClicked = null;
  });

  function pendingUpdates() {
    return pendingReconcileUpdates(candidates, checked);
  }

  let dirty = $derived(pendingUpdates().length > 0);

  let amountByGuid = $derived(new Map(candidates.map((c) => [c.splitGuid, c.amount] as const)));
  let cleared = $derived(clearedTotal([...checked].map((g) => amountByGuid.get(g) ?? 0)));
  let target = $derived(targetBalance(startingBalance, cleared));
  let endingBalance = $derived(parseStatementAmount(endingBalanceInput, appSettings.value.locale));
  let balanceIssue = $derived.by<'empty' | 'invalid' | 'none'>(() => {
    if (endingBalanceInput.trim() === '') return 'empty';
    return endingBalance === null ? 'invalid' : 'none';
  });
  let difference = $derived(
    endingBalance === null ? NaN : statementDifference(endingBalance, target)
  );
  let balanced = $derived(endingBalance !== null && isBalanced(difference));
  let saving = $state(false);
  let finishing = $state(false);
  // Finishing promotes every cleared split, so the header button opens a
  // confirmation instead of committing straight away.
  let confirmFinishOpen = $state(false);
  // Finish sends unsaved marks with the same call, so dirty must not block it
  // — only a missing/invalid ending balance, an unbalanced statement, or an
  // empty selection does.
  let canFinish = $derived(
    !!res.data && endingBalance !== null && balanced && !saving && !finishing && checked.size > 0
  );

  // Jump straight to the candidate(s) that explain an off-by amount.
  let amountFilter = $state<number | null>(null);
  let searchAmount = $derived.by(() => {
    const q = search.value.trim();
    return q ? parseStatementAmount(q, appSettings.value.locale) : null;
  });

  let filtered = $derived(
    filterAndSortCandidates(candidates, {
      query: search.value,
      queryAmount: searchAmount,
      amountFilter,
      sortBy: sort.sortBy,
      sortDir: sort.sortDir
    })
  );

  let hiddenCount = $derived(candidates.length - filtered.length);
  let duplicates = $derived(duplicateCandidateGuids(candidates));

  function toggle(splitGuid: string, next: boolean) {
    if (next) checked.add(splitGuid);
    else checked.delete(splitGuid);
    lastClicked = splitGuid;
  }

  function toggleAll(next: boolean) {
    for (const row of filtered) {
      if (next) checked.add(row.splitGuid);
      else checked.delete(row.splitGuid);
    }
    lastClicked = null;
  }

  /** Shift-click range: set every row between the last click and this one. */
  function rangeSelect(splitGuid: string) {
    const guids = rangeSplitGuids(filtered, lastClicked, splitGuid);
    if (guids.length === 0) return;
    const next = !checked.has(splitGuid);
    for (const g of guids) {
      if (next) checked.add(g);
      else checked.delete(g);
    }
    lastClicked = splitGuid;
  }

  // ── draft persistence ─────────────────────────────────────────────────────

  let draftFor = $state<string | null>(null);

  $effect(() => {
    const g = guid;
    if (!g || draftFor === g) return;
    draftFor = g;
    const draft = readReconcileDraft(g);
    if (!draft) return;
    endingBalanceInput = draft.endingBalance;
    // A `statement` URL param was shared/bookmarked explicitly, so it wins.
    if (draft.statementDate && !page.url.searchParams.has('statement')) {
      statementDate = draft.statementDate;
    }
  });

  $effect(() => {
    const g = draftFor;
    const balance = endingBalanceInput;
    const date = statementDate;
    if (!g) return;
    // Don't litter storage with pristine accounts.
    if (balance.trim() === '' && date === today.value) {
      removeReconcileDraft(g);
      return;
    }
    writeReconcileDraft(g, { endingBalance: balance, statementDate: date });
  });

  function clearDraft(g: string) {
    removeReconcileDraft(g);
    // Reset to the pristine form before releasing the key: the read effect would
    // otherwise re-seed `draftFor` and the write effect would immediately save
    // the still-populated inputs back to storage.
    endingBalanceInput = '';
    statementDate = today.value;
    draftFor = g;
  }

  // ── leaving with unsaved marks ────────────────────────────────────────────

  let leaveOpen = $state(false);
  let pendingLeaveUrl: string | null = null;
  let bypassGuard = $state(false);
  /** True when the prompt came from the window close button, not navigation. */
  let closeRequested = $state(false);

  // Closing the window with unsaved marks must prompt. SvelteKit's
  // `willUnload` has no native confirmation, so hook Tauri's close-requested
  // event (and `beforeunload` for the browser dev shell).
  $effect(() => {
    if (!dirty || bypassGuard) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    // Guard the Tauri-only call: getCurrentWindow() throws synchronously on a
    // plain-browser host, and a throwing effect corrupts Svelte's effect tree.
    if (isTauri()) {
      void getCurrentWindow()
        .onCloseRequested((event) => {
          event.preventDefault();
          pendingLeaveUrl = null;
          closeRequested = true;
          leaveOpen = true;
        })
        .then((fn) => {
          if (disposed) fn();
          else unlisten = fn;
        })
        .catch(() => {
          // Registration failed; the beforeunload listener below still prompts.
        });
    }
    const beforeUnload = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', beforeUnload);
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener('beforeunload', beforeUnload);
    };
  });

  beforeNavigate((nav) => {
    if (bypassGuard || saving || !dirty) return;
    // Same-page query refinements (search, statement date) aren't leaving.
    const to = nav.to;
    if (to && to.route.id === page.route.id && to.url.pathname === page.url.pathname) return;
    if (nav.type === 'leave' || nav.willUnload) {
      // Window closes are handled by the close-requested effect; external links
      // fall back to the browser's unload prompt.
      nav.cancel();
      return;
    }
    if (!to) return;
    pendingLeaveUrl = to.url.href;
    leaveOpen = true;
    nav.cancel();
  });

  async function leaveWithoutSaving() {
    leaveOpen = false;
    bypassGuard = true;
    if (closeRequested) {
      await getCurrentWindow().destroy();
      return;
    }
    // The target is an already-resolved href captured from beforeNavigate.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    await goto(pendingLeaveUrl ?? resolve('/reconcile'));
  }

  async function saveAndLeave() {
    const ok = await save();
    if (!ok) return; // Stay put with the error toast and the marks intact.
    leaveOpen = false;
    bypassGuard = true;
    if (closeRequested) {
      await getCurrentWindow().destroy();
      return;
    }
    // The target is an already-resolved href captured from beforeNavigate.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    await goto(pendingLeaveUrl ?? resolve('/reconcile'));
  }

  // ── actions ───────────────────────────────────────────────────────────────

  async function save(): Promise<boolean> {
    const g = guid;
    if (!g) return false;
    const updates = pendingUpdates();
    if (updates.length === 0) return true;
    saving = true;
    try {
      await markReconcileSplits(g, statementDate, updates);
      // Saving progress doesn’t move the summary counters; refresh the session
      // so nav roots and badges pick up any changes.
      refreshSession();
      toast.success('Progress saved');
      res.reload();
      return true;
    } catch (e) {
      toast.error(errMsg(e, 'We couldn’t save your progress'));
      return false;
    } finally {
      saving = false;
    }
  }

  async function finish() {
    const g = guid;
    if (!g || endingBalance === null) return;
    finishing = true;
    try {
      // Unsaved marks ride along with Finish so the balance check, the update
      // and the promotion commit (or roll back) as one transaction.
      const updates = pendingUpdates();
      const out = await finishReconciliation(g, statementDate, endingBalance, updates);
      toast.success(`Reconciled ${plural(out.reconciledCount, 'split')}`);
      clearDraft(g);
      bypassGuard = true;
      void goto(resolve('/reconcile'));
    } catch (e) {
      toast.error(errMsg(e, 'We couldn’t finish reconciling'));
    } finally {
      finishing = false;
    }
  }

  let undoOpen = $state(false);
  let undoing = $state(false);

  async function undoLastReconciliation() {
    const g = guid;
    if (!g) return;
    undoing = true;
    try {
      const out = await reopenLastReconciliation(g);
      toast.success(`Reopened ${plural(out.reopenedCount, 'split')}`);
      undoOpen = false;
      res.reload();
    } catch (e) {
      toast.error(errMsg(e, 'We couldn’t reopen the last statement'));
    } finally {
      undoing = false;
    }
  }

  function onSort(key: TxSortKey) {
    if (key === 'date' || key === 'description' || key === 'amount') sort.toggle(key);
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (dirty && !saving && !finishing) void save();
    }
  }
</script>

<svelte:head>
  <title>{res.data ? `Reconcile ${res.data.accountName} · Spendo` : 'Reconcile · Spendo'}</title>
  <meta name="description" content="Tick cleared transactions until the difference is zero" />
</svelte:head>

<svelte:window onkeydown={onWindowKeydown} />

<BookGate title="Reconcile" description="Tick cleared transactions until the difference is zero">
  <div class="page-stack">
    <PageHeader
      title={res.data ? `Reconcile ${res.data.accountName}` : 'Reconcile'}
      breadcrumbs={[{ label: PAGE_META.reconcile.title, href: resolve('/reconcile') }]}
    >
      {#snippet description()}
        {#if res.data?.accountDescription}
          {res.data.accountDescription} &middot;
        {/if}
        Tick cleared transactions until the difference is zero, then finish.
      {/snippet}
      <Button
        variant="outline"
        disabled={!dirty || saving || finishing}
        title="Save progress (Ctrl+S)"
        onclick={() => void save()}
      >
        {saving ? 'Saving…' : 'Save progress'}
      </Button>
      <Button
        disabled={!canFinish || saving || finishing}
        onclick={() => (confirmFinishOpen = true)}
      >
        {finishing ? 'Finishing…' : 'Finish reconciliation'}
      </Button>
      {#if res.data && res.data.lastReconciledDate && res.data.lastReconciledCount > 0}
        <OverflowMenu
          actions={[
            { label: 'Undo last statement', icon: Undo2, onSelect: () => (undoOpen = true) }
          ]}
        />
      {/if}
    </PageHeader>

    {#if (res.loading || dbBootstrapping.value) && !res.data && !res.error}
      <PageSkeleton variant="reconcile-detail" />
    {:else if res.data && !res.error}
      <DimWrapper active={res.loading} class="flex flex-col gap-6">
        <ReconcileKpiCards
          {currency}
          {startingBalance}
          {cleared}
          {target}
          {difference}
          {balanced}
          onFindDifference={() => {
            amountFilter = difference;
          }}
        />

        <ReconcileStatementForm
          bind:statementDate
          bind:endingBalanceInput
          {balanceIssue}
          checkedCount={checked.size}
          totalCount={candidates.length}
          {dirty}
          {balanced}
          lastReconciledDate={res.data.lastReconciledDate}
          maxDate={today.value}
        />

        <div class="flex flex-col gap-4">
          <FilterBar>
            <SearchInput
              id="reconcile-search"
              label="Search description, number, memo or amount"
              placeholder="Search description, number, memo or amount…"
              bind:value={searchInput}
              class="grow basis-48"
            />
            {#if amountFilter !== null}
              <Button
                variant="outline"
                title="Clear the amount filter"
                onclick={() => (amountFilter = null)}
              >
                Amount = {formatSignedCurrency(amountFilter, currency)}
                <X aria-hidden="true" class="size-3.5" />
              </Button>
            {/if}
            {#if search.value}
              <FilterClearButton
                onclick={() => {
                  searchInput = '';
                  search.set('');
                }}
              />
            {/if}
          </FilterBar>

          {#if hiddenCount > 0}
            <p class="text-sm text-muted-foreground">
              {plural(hiddenCount, 'transaction')} hidden by filters. “Mark all” only covers the
              {filtered.length} shown.
            </p>
          {/if}

          {#if candidates.length > 500}
            <p class="text-sm text-muted-foreground">
              This is a long list ({candidates.length} candidates). Reconciling an earlier statement date
              first keeps it manageable.
            </p>
          {/if}

          {#if filtered.length === 0}
            {#if search.value || amountFilter !== null}
              <FilteredEmptyState
                title="No matching transactions"
                onClear={() => {
                  searchInput = '';
                  search.set('');
                  amountFilter = null;
                }}
              />
            {:else}
              <EmptyState
                card
                title="Nothing to reconcile"
                description="No unreconciled transactions at or before the statement date."
              >
                {#snippet icon()}
                  <CircleCheck aria-hidden="true" class="text-muted-foreground" />
                {/snippet}
              </EmptyState>
            {/if}
          {:else}
            <ReconcileTable
              rows={filtered}
              {checked}
              {currency}
              {duplicates}
              sortBy={sort.sortBy}
              sortDir={sort.sortDir}
              {onSort}
              onToggle={toggle}
              onToggleAll={toggleAll}
              onRangeSelect={rangeSelect}
            />
          {/if}
        </div>
      </DimWrapper>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if res.loading}
      <PageSkeleton variant="reconcile-detail" />
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

<ReconcileLeaveDialog
  bind:open={leaveOpen}
  {closeRequested}
  {saving}
  onStay={() => {
    leaveOpen = false;
    closeRequested = false;
  }}
  onLeave={leaveWithoutSaving}
  onSaveAndLeave={saveAndLeave}
/>

<ReconcileFinishDialog
  bind:open={confirmFinishOpen}
  checkedCount={checked.size}
  {finishing}
  onConfirm={() => void finish()}
/>

{#if res.data && res.data.lastReconciledDate && res.data.lastReconciledCount > 0}
  <ReconcileUndoDialog
    bind:open={undoOpen}
    lastReconciledCount={res.data.lastReconciledCount}
    lastReconciledDate={res.data.lastReconciledDate}
    {undoing}
    onConfirm={undoLastReconciliation}
  />
{/if}
