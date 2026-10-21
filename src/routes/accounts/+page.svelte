<script lang="ts">
  import { ChevronsDownUp, ChevronsUpDown, Landmark, Plus } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
  import FilteredEmptyState from '$lib/components/shared/filtered-empty-state.svelte';
  import { SearchInput } from '$lib/components/shared/search-input';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import AccountTreeTable from '$lib/features/accounts/account-tree-table.svelte';
  import {
    buildAccountSections,
    parentGuids,
    visibleAccountRows
  } from '$lib/features/accounts/account-tree';
  import { openAccountDialog } from '$lib/features/accounts/account-dialog-state.svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { debouncedSearchParam, useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { isStringArray, usePersistedState } from '$lib/hooks/use-persisted-state.svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/tauri';
  import { plural } from '$lib/utils/format.svelte';

  const meta = PAGE_META.accounts;

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
    fetch: () => {
      // Refetch when an account is created or deleted anywhere (the layout’s
      // account dialog refreshes the session summary).
      void dbSession.value?.summary.totalAccounts;
      return fetchAccounts();
    },
    label: 'We couldn’t load your accounts'
  });

  const expandedStore = usePersistedState<string[]>('tree:accounts', [], isStringArray);
  let expanded = $derived(new Set(expandedStore.value));

  let sections = $derived(res.data ? buildAccountSections(res.data) : []);
  let allParents = $derived(sections.flatMap((s) => parentGuids(s.roots)));
  let allExpanded = $derived(allParents.length > 0 && allParents.every((g) => expanded.has(g)));
  let hasMatches = $derived(
    sections.some((s) => visibleAccountRows(s.roots, expanded, search.value).length > 0)
  );

  function toggle(guid: string) {
    const current = expandedStore.value;
    expandedStore.value = current.includes(guid)
      ? current.filter((g) => g !== guid)
      : [...current, guid];
  }

  function clearSearch() {
    searchInput = '';
    search.set('');
  }
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader
      title={meta.title}
      description={res.data
        ? `${meta.description} · ${plural(res.data.totalAccounts, 'account')}`
        : meta.description}
    >
      {#if dbSession.value}
        <Button onclick={openAccountDialog}>
          <Plus aria-hidden="true" class="size-4" />
          New account
        </Button>
      {/if}
    </PageHeader>

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <TableSkeleton cols={4} rows={8} />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if sections.length === 0}
        <EmptyState
          card
          title="No accounts yet"
          description="Add your bank, card and spending accounts, or import a GnuCash book."
        >
          {#snippet icon()}
            <Landmark aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
          {#snippet action()}
            <Button onclick={openAccountDialog}>
              <Plus aria-hidden="true" class="size-4" />
              New account
            </Button>
          {/snippet}
        </EmptyState>
      {:else}
        <FilterBar>
          <SearchInput
            id="accounts-search"
            label="Search accounts"
            placeholder="Search accounts…"
            bind:value={searchInput}
            class="grow basis-48"
          />
          {#if search.value}
            <FilterClearButton onclick={clearSearch} />
          {/if}
          {#snippet extra()}
            <Button
              variant="outline"
              disabled={!!search.value || allParents.length === 0}
              onclick={() => (expandedStore.value = allExpanded ? [] : allParents)}
            >
              {#if allExpanded}
                <ChevronsDownUp aria-hidden="true" class="size-4" />
                Collapse all
              {:else}
                <ChevronsUpDown aria-hidden="true" class="size-4" />
                Expand all
              {/if}
            </Button>
          {/snippet}
        </FilterBar>

        {#if hasMatches}
          <DimWrapper active={res.loading}>
            <AccountTreeTable
              {sections}
              {expanded}
              query={search.value}
              reportCurrency={res.data.reportCurrency}
              onToggle={toggle}
            />
          </DimWrapper>
        {:else}
          <FilteredEmptyState title="No matching accounts" onClear={clearSearch} />
        {/if}
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
