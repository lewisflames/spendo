<script lang="ts">
  import { Scale } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/tauri';
  import { resolve } from '$app/paths';
  import { plural } from '$lib/utils/format.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import FilteredEmptyState from '$lib/components/shared/filtered-empty-state.svelte';
  import { Button } from '$lib/components/ui/button';
  import { SearchInput } from '$lib/components/shared/search-input';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
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
    label: "We couldn't load your accounts"
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
  <title>Reconcile · Spendo</title>
  <meta name="description" content="Match your books to a bank statement." />
</svelte:head>

<BookGate title="Reconcile" description="Match your books to the bank">
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Reconcile"
      description={res.data
        ? `Match your books to the bank · ${plural(filtered.length, 'account you can reconcile')}`
        : 'Match your books to the bank'}
    ></PageHeader>

    <FilterBar>
      <SearchInput
        id="reconcile-account-search"
        label="Search accounts"
        placeholder="Search accounts…"
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

    {#if res.loading && !res.data}
      <PageSkeleton variant="reconcile" />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if filtered.length === 0}
        {#if search.value}
          <FilteredEmptyState
            title="No matching accounts"
            onClear={() => {
              searchInput = '';
              search.set('');
            }}
          />
        {:else}
          <EmptyState
            card
            title="Nothing to reconcile yet"
            description="Your cash, bank and credit accounts will land here once you’ve added them."
          >
            {#snippet icon()}
              <Scale aria-hidden="true" class="text-muted-foreground" />
            {/snippet}
            {#snippet action()}
              <Button variant="outline" href={resolve('/accounts')}>Accounts</Button>
            {/snippet}
          </EmptyState>
        {/if}
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
