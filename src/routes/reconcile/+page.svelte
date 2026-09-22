<script lang="ts">
  import { Scale, X } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/tauri';
  import { plural } from '$lib/utils/format.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { Button } from '$lib/components/ui/button';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useUrlParams, debouncedSearchParam } from '$lib/hooks/use-url-params.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import ReconcileAccountTable from '$lib/features/reconcile/reconcile-account-table.svelte';
  import { flattenReconcilableAccounts } from '$lib/features/reconcile/reconcile-accounts';

  let searchInput = $state('');
  let search = useDebouncedSearch(() => searchInput);

  useUrlParams([
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
    fetch: () => fetchAccounts(),
    label: 'Failed to load accounts'
  });

  let rows = $derived(flattenReconcilableAccounts(res.data?.trees));
  let filtered = $derived.by(() => {
    const q = search.value.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(
      (r) =>
        r.label.toLowerCase().includes(q) ||
        r.accountType.toLowerCase().includes(q) ||
        (r.code?.toLowerCase().includes(q) ?? false)
    );
  });
</script>

<svelte:head>
  <title>Reconcile — Spendo</title>
  <meta name="description" content="Pick an account to reconcile against a bank statement." />
</svelte:head>

<BookGate title="Reconcile">
  <div class="flex flex-col gap-6">
    <PageHeader title="Reconcile">
      {#snippet description()}
        {#if res.data}
          {plural(filtered.length, 'reconcilable account')}
        {:else}
          Pick an account to reconcile against a bank statement.
        {/if}
      {/snippet}
    </PageHeader>

    <div class="flex flex-wrap items-end gap-2">
      <SearchInput
        id="reconcile-account-search"
        label="Filter accounts"
        placeholder="Filter accounts…"
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
          <X aria-hidden="true" class="mr-1 size-3" />
          Clear
        </Button>
      {/if}
    </div>

    {#if res.loading && !res.data}
      <TableSkeleton cols={4} rows={8} />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if filtered.length === 0}
        <EmptyState
          card
          title="No reconcilable accounts"
          description={search.value
            ? 'No reconcilable accounts match the current filter.'
            : 'Cash, bank and credit accounts show up here once your book has accounts.'}
        >
          {#snippet icon()}
            <Scale aria-hidden="true" class="text-muted-foreground" />
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
        <DimWrapper active={res.loading}>
          <ReconcileAccountTable rows={filtered} />
        </DimWrapper>
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
