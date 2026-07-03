<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { Database, ChevronRight, Search } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/accounts.remote';
  import EmptyState from '$lib/components/empty-state.svelte';
  import { formatNumber as fmtCount, formatCurrency as fmtCurrency } from '$lib/format.svelte';
  import { cn } from '$lib/utils.js';
  import { TYPE_CATEGORY } from '$lib/account-types';
  import type { AccountsResult, AccountNode } from '$lib/server/accounts';

  function balanceColor(balance: number, category: string): string {
    if (category === 'LIABILITY' || category === 'EQUITY') {
      return balance >= 0
        ? 'text-amber-600 dark:text-amber-400'
        : 'text-emerald-600 dark:text-emerald-400';
    }
    if (category === 'INCOME') return 'text-emerald-600 dark:text-emerald-400';
    if (category === 'EXPENSE') return 'text-red-600 dark:text-red-400';
    return balance >= 0
      ? 'text-emerald-600 dark:text-emerald-400'
      : 'text-red-600 dark:text-red-400';
  }

  function nodeCategory(node: AccountNode): string {
    return TYPE_CATEGORY[node.accountType] ?? node.accountType;
  }

  let data = $state<AccountsResult | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let expanded = $state<string[]>([]);
  let searchQuery = $state('');

  let allNodeGuids = $derived.by(() => {
    const d = data;
    if (!d) return [];
    const guids: string[] = [];
    function walk(nodes: AccountNode[]) {
      for (const n of nodes) {
        if (n.children.length > 0) guids.push(n.guid);
        walk(n.children);
      }
    }
    walk(d.trees);
    return guids;
  });

  let hasExpandableNodes = $derived(allNodeGuids.length > 0);

  function toggle(guid: string) {
    if (expanded.includes(guid)) {
      expanded = expanded.filter((g) => g !== guid);
    } else {
      expanded = [...expanded, guid];
    }
  }

  function expandAll() {
    expanded = [...allNodeGuids];
  }

  function collapseAll() {
    expanded = [];
  }

  function fmt(n: number): string {
    return fmtCurrency(n, dbSession.value?.summary.currency ?? 'EUR');
  }

  function flattenByType(
    nodes: AccountNode[],
    type: string,
    depth = 0
  ): { node: AccountNode; depth: number }[] {
    const result: { node: AccountNode; depth: number }[] = [];
    for (const n of nodes) {
      if (n.accountType === type || depth > 0) {
        result.push({ node: n, depth });
        if (n.children.length > 0 && (expanded.includes(n.guid) || searchQuery)) {
          result.push(...flattenByType(n.children, type, depth + 1));
        }
      } else {
        result.push(...flattenByType(n.children, type, depth));
      }
    }
    return result;
  }

  let grouped = $derived.by(() => {
    const d = data;
    if (!d) return [];
    return Object.keys(d.byType).map((type) => ({
      type,
      info: d.byType[type],
      accounts: flattenByType(d.trees, type).filter(({ node }) => {
        if (!searchQuery) return true;
        const q = searchQuery.toLowerCase();
        return (
          node.name.toLowerCase().includes(q) || (node.code && node.code.toLowerCase().includes(q))
        );
      })
    }));
  });

  $effect(() => {
    if (!dbSession.value) {
      data = null;
      loading = false;
      error = null;
      return;
    }
    loading = true;
    error = null;
    data = null;

    let cancelled = false;

    fetchAccounts({ token: dbSession.value.token })
      .then((result) => {
        if (cancelled) return;
        data = result;
        expanded = [...allNodeGuids];
        loading = false;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error = e instanceof Error ? e.message : 'Failed to load accounts';
        loading = false;
      });

    return () => {
      cancelled = true;
    };
  });
</script>

<svelte:head>
  <title>Accounts - Spendo</title>
</svelte:head>

{#if !dbSession.value}
  <EmptyState title="No Data" description="Upload a GnuCash file to see your accounts.">
    {#snippet icon()}
      <Database aria-hidden="true" class="size-12 text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <div class="flex flex-col gap-6">
    <div class="flex flex-col gap-1">
      <h2 class="text-3xl font-bold tracking-tight">Accounts</h2>
      {#if data}
        <p class="text-sm text-muted-foreground">
          {fmtCount(data.totalAccounts)} account{data.totalAccounts === 1 ? '' : 's'}
        </p>
      {:else if loading}
        <Skeleton class="h-4 w-32" />
      {/if}
    </div>

    {#if loading}
      <!-- Summary card skeletons -->
      <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {#each Array(4) as _, i (i)}
          <Card.Root>
            <Card.Header>
              <div class="flex items-center gap-2">
                <Skeleton class="h-4 w-20" />
                <Skeleton class="h-5 w-8 rounded-full" />
              </div>
            </Card.Header>
            <Card.Content>
              <Skeleton class="h-7 w-28" />
            </Card.Content>
          </Card.Root>
        {/each}
      </div>

      <!-- Account group skeletons -->
      {#each Array(2) as _, i (i)}
        <Card.Root>
          <Card.Header>
            <Skeleton class="h-5 w-24" />
            <Skeleton class="mt-1 h-3 w-36" />
          </Card.Header>
          <Card.Content class="p-0">
            <div class="divide-y">
              {#each Array(5) as _, i (i)}
                <div class="flex items-center gap-2 px-4 py-2">
                  <Skeleton class="size-3.5 shrink-0 rounded" />
                  <Skeleton class="h-4 flex-1 max-w-40" />
                  <Skeleton class="ml-auto h-4 w-20" />
                </div>
              {/each}
            </div>
          </Card.Content>
        </Card.Root>
      {/each}
    {:else if error}
      <Card.Root class="border-destructive">
        <Card.Content class="py-6 text-center text-sm text-destructive">
          {error}
        </Card.Content>
      </Card.Root>
    {:else if data}
      <!-- Search and controls -->
      <div class="flex items-center gap-2">
        <div class="relative grow">
          <Search
            aria-hidden="true"
            class="text-muted-foreground pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2"
          />
          <Input class="pl-8" placeholder="Search accounts..." bind:value={searchQuery} />
        </div>
        {#if hasExpandableNodes}
          <Button
            variant="ghost"
            size="sm"
            onclick={expandAll}
            disabled={expanded.length === allNodeGuids.length}
          >
            Expand all
          </Button>
          <Button variant="ghost" size="sm" onclick={collapseAll} disabled={expanded.length === 0}>
            Collapse all
          </Button>
        {/if}
      </div>

      <!-- Summary cards by type -->
      <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {#each Object.entries(data.byType) as [type, info] (type)}
          <Card.Root>
            <Card.Header>
              <div class="flex items-center gap-2">
                <Card.Title>{type}</Card.Title>
                <Badge variant="secondary">{fmtCount(info.count)}</Badge>
              </div>
            </Card.Header>
            <Card.Content>
              <p class="text-2xl font-bold tabular-nums {balanceColor(info.totalBalance, type)}">
                {info.totalBalance >= 0 ? '+' : ''}{fmt(info.totalBalance)}
              </p>
            </Card.Content>
          </Card.Root>
        {/each}
      </div>

      <!-- Account groups by type -->
      {#each grouped as group (group.type)}
        <Card.Root>
          <Card.Header>
            <Card.Title>{group.type}</Card.Title>
            <Card.Description>
              {fmtCount(group.info.count)} account{group.info.count === 1 ? '' : 's'} &middot; {fmt(
                group.info.totalBalance
              )}
            </Card.Description>
          </Card.Header>
          <Card.Content class="p-0">
            <div class="divide-y">
              {#each group.accounts as { node, depth } (node.guid)}
                <div
                  class={cn(
                    'flex w-full items-center gap-1.5 px-4 py-2 text-sm transition-colors hover:bg-muted',
                    expanded.includes(node.guid) && 'bg-muted/50'
                  )}
                  style="padding-left: {1 + depth * 1.25}rem"
                >
                  {#if node.children.length > 0}
                    <button
                      type="button"
                      class="flex size-5 shrink-0 items-center justify-center rounded hover:bg-muted-foreground/10"
                      onclick={() => toggle(node.guid)}
                    >
                      <ChevronRight
                        aria-hidden="true"
                        class="size-3.5 text-muted-foreground transition-transform {expanded.includes(
                          node.guid
                        )
                          ? 'rotate-90'
                          : ''}"
                      />
                    </button>
                  {:else}
                    <div class="size-5 shrink-0"></div>
                  {/if}
                  <a
                    href={resolve(`/accounts/${node.guid}`)}
                    class="flex min-w-0 flex-1 items-center gap-2 text-left"
                  >
                    <span class="flex-1 truncate font-medium">{node.name}</span>
                    {#if node.placeholder}
                      <Badge
                        variant="outline"
                        class="shrink-0 text-[10px] leading-none text-muted-foreground"
                      >
                        PL
                      </Badge>
                    {/if}
                    {#if node.code}
                      <span class="hidden sm:inline text-xs text-muted-foreground">{node.code}</span
                      >
                    {/if}
                    <span
                      class="tabular-nums text-xs {balanceColor(node.balance, nodeCategory(node))}"
                    >
                      {fmt(node.balance)}
                    </span>
                  </a>
                </div>
              {/each}
            </div>
          </Card.Content>
        </Card.Root>
      {/each}
    {/if}
  </div>
{/if}
