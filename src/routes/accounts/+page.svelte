<script lang="ts">
  import { SearchInput } from '$lib/components/shared/search-input';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import {
    UnfoldVertical,
    FoldVertical,
    FolderTree,
    Landmark,
    PiggyBank,
    CreditCard,
    Download
  } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/components/ui/sonner';
  import { downloadCsv } from '$lib/utils/csv';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/tauri';
  import type { AccountNode } from '$lib/tauri';
  import {
    CATEGORY_ORDER,
    TYPE_CATEGORY,
    nodeCategory
  } from '$lib/features/accounts/account-types';
  import { useTreeExpand } from '$lib/hooks/use-tree-expand.svelte';
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import { clampZero, formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import AccountsTreeTable from '$lib/features/accounts/accounts-tree-table.svelte';
  import BalanceTreemap from '$lib/features/accounts/balance-treemap.svelte';

  const CATEGORIES = ['All', ...CATEGORY_ORDER];

  const CATEGORY_LABEL: Record<string, string> = {
    All: 'All',
    ASSET: 'Assets',
    LIABILITY: 'Liabilities',
    EQUITY: 'Equity',
    INCOME: 'Income',
    EXPENSE: 'Expenses'
  };

  type Category = 'ASSET' | 'EQUITY' | 'LIABILITY';

  const KPI_CATEGORIES: Category[] = ['ASSET', 'EQUITY', 'LIABILITY'];

  const KPI_ICONS: Record<Category, typeof Landmark> = {
    ASSET: Landmark,
    EQUITY: PiggyBank,
    LIABILITY: CreditCard
  };

  const KPI_ICON_COLORS: Record<Category, string> = {
    ASSET: 'text-success',
    EQUITY: 'text-warning',
    LIABILITY: 'text-destructive'
  };

  const KPI_VALUE_COLORS: Record<Category, string> = {
    ASSET: 'text-success',
    EQUITY: 'text-warning',
    LIABILITY: 'text-destructive'
  };

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchAccounts,
    label: 'Failed to load accounts'
  });

  let data = $derived(res.data);
  let loading = $derived(res.loading);
  let error = $derived(res.error);

  let searchInput = $state('');
  let category = $state('All');

  let search = useDebouncedSearch(
    () => searchInput,
    (v) => v.trim().toLowerCase()
  );

  function categoryOf(node: AccountNode): string {
    return TYPE_CATEGORY[node.accountType] ?? node.accountType;
  }

  function matchesQuery(node: AccountNode, q: string): boolean {
    return [node.name, node.description, node.code].some((s) => s?.toLowerCase().includes(q));
  }

  function filterQuery(nodes: AccountNode[], q: string): AccountNode[] {
    const out: AccountNode[] = [];
    for (const n of nodes) {
      if (matchesQuery(n, q)) {
        out.push(n);
      } else {
        const kids = filterQuery(n.children, q);
        if (kids.length > 0) out.push({ ...n, children: kids });
      }
    }
    return out;
  }

  function filterCategory(nodes: AccountNode[], cat: string): AccountNode[] {
    if (cat === 'All') return nodes;
    const out: AccountNode[] = [];
    for (const n of nodes) {
      if (categoryOf(n) === cat) {
        out.push(n);
      } else {
        out.push(...filterCategory(n.children, cat));
      }
    }
    return out;
  }

  let filteredTrees = $derived.by(() => {
    let trees = data?.trees ?? [];
    if (search.value) trees = filterQuery(trees, search.value);
    trees = filterCategory(trees, category);
    return trees;
  });

  let tree = useTreeExpand();
  let initialExpandDone = $state(false);

  $effect(() => {
    if (data && !initialExpandDone) {
      // Expand everything on the first load of a session only; later
      // refetches must keep the user's manual collapse choices.
      tree.expandAll(parentGuids);
      initialExpandDone = true;
    }
  });

  $effect(() => {
    if (!dbSession.value) {
      initialExpandDone = false;
    }
  });

  let parentGuids = $derived.by(() => {
    const guids: string[] = [];
    function walk(nodes: AccountNode[]) {
      for (const n of nodes) {
        if (n.children.length > 0) guids.push(n.guid);
        walk(n.children);
      }
    }
    walk(filteredTrees);
    return guids;
  });

  let rows = $derived.by(() => {
    const result: { node: AccountNode; depth: number }[] = [];
    function walk(nodes: AccountNode[], depth: number) {
      for (const n of nodes) {
        result.push({ node: n, depth });
        if (n.children.length > 0 && (search.value || depth === 0 || tree.expanded.has(n.guid))) {
          walk(n.children, depth + 1);
        }
      }
    }
    walk(filteredTrees, 0);
    return result;
  });

  function categoryTotal(cat: Category): number {
    return data?.byType[cat]?.totalBalance ?? 0;
  }

  let kpiCards = $derived.by(() => {
    if (!data) return [];
    return KPI_CATEGORIES.map((cat) => ({
      key: cat,
      title: CATEGORY_LABEL[cat],
      value: categoryTotal(cat),
      icon: KPI_ICONS[cat],
      iconClass: KPI_ICON_COLORS[cat],
      valueClass: KPI_VALUE_COLORS[cat]
    }));
  });

  function handleExportCsv() {
    if (rows.length === 0) return;
    try {
      const headers = ['Category', 'Account', 'Type', 'Txn Count', 'Depth', 'Balance'];
      const csvRows = rows.map(({ node, depth }) => [
        nodeCategory(node.accountType),
        node.description || node.name,
        node.accountType,
        String(node.txCount),
        String(depth),
        String(node.balance)
      ]);
      downloadCsv('accounts.csv', headers, csvRows);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to export accounts'));
    }
  }

  function displayValue(n: number): string {
    return fmt(clampZero(n));
  }
</script>

<svelte:head>
  <title>Accounts — Spendo</title>
  <meta name="description" content="Chart of accounts." />
</svelte:head>

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Accounts"
      description={data
        ? `Chart of accounts · ${data.totalAccounts} account${data.totalAccounts === 1 ? '' : 's'}`
        : ''}
    >
      <Button variant="outline" onclick={handleExportCsv} disabled={rows.length === 0}>
        <Download aria-hidden="true" class="size-3" />
        Export
      </Button>
    </PageHeader>

    {#if loading && !data}
      <KpiCardRow />

      <TableSkeleton cols={4} />
    {:else if error}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {:else if data}
      <!-- Summary cards by account category -->
      <DimWrapper active={loading} class="grid grid-cols-1 gap-6 sm:grid-cols-3">
        {#each kpiCards as card (card.key)}
          <KpiCard title={card.title} value={displayValue(card.value)} valueClass={card.valueClass}>
            {#snippet icon()}
              {@const Icon = card.icon}
              <Icon aria-hidden="true" class="size-4 {card.iconClass}" />
            {/snippet}
          </KpiCard>
        {/each}
      </DimWrapper>

      <!-- Balance composition treemap -->
      <BalanceTreemap data={data?.trees ?? []} {loading} />

      <div class="flex flex-wrap items-center gap-2">
        <SearchInput
          id="accounts-search"
          label="Search accounts"
          placeholder="Search name, description, or code..."
          bind:value={searchInput}
          class="grow basis-48"
        />

        <div class="flex gap-1">
          {#each CATEGORIES as cat (cat)}
            <button
              type="button"
              class="rounded-md px-2 py-1 text-xs font-medium transition-colors {category === cat
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:bg-muted'}"
              aria-pressed={category === cat}
              onclick={() => (category = cat)}
            >
              {CATEGORY_LABEL[cat]}
            </button>
          {/each}
        </div>

        <div class="flex gap-1">
          <Tooltip.Root>
            <Tooltip.Trigger>
              {#snippet child({ props })}
                <button
                  type="button"
                  {...props}
                  class="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  aria-label="Expand all"
                  disabled={parentGuids.length === 0}
                  onclick={() => tree.expandAll(parentGuids)}
                >
                  <UnfoldVertical class="size-4" />
                </button>
              {/snippet}
            </Tooltip.Trigger>
            <Tooltip.Content>
              <p>Expand all</p>
            </Tooltip.Content>
          </Tooltip.Root>
          <Tooltip.Root>
            <Tooltip.Trigger>
              {#snippet child({ props })}
                <button
                  type="button"
                  {...props}
                  class="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  aria-label="Collapse all"
                  onclick={() => tree.collapseAll()}
                >
                  <FoldVertical class="size-4" />
                </button>
              {/snippet}
            </Tooltip.Trigger>
            <Tooltip.Content>
              <p>Collapse all</p>
            </Tooltip.Content>
          </Tooltip.Root>
        </div>
      </div>

      {#if rows.length === 0}
        <EmptyState
          card
          title="No accounts found"
          description="No accounts match the current filters."
        >
          {#snippet icon()}
            <FolderTree aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
        </EmptyState>
      {:else}
        <DimWrapper active={loading}>
          <AccountsTreeTable {rows} {tree} searchActive={!!search.value} />
        </DimWrapper>
      {/if}
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Accounts" description="Chart of accounts" />
    <ImportPrompt />
  </div>
{/if}
