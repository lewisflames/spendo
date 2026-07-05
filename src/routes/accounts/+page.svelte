<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { ChevronRight, Search } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/accounts.remote';
  import { goto } from '$app/navigation';
  import { formatNumber as fmtCount, formatCurrency as fmtCurrency } from '$lib/format.svelte';
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
  let expandedGroups = $state<string[]>([]);
  let searchQuery = $state('');
  let initialExpandDone = $state(false);

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

  function toggleGroup(type: string) {
    if (expandedGroups.includes(type)) {
      expandedGroups = expandedGroups.filter((g) => g !== type);
    } else {
      expandedGroups = [...expandedGroups, type];
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

  type TableRow =
    | { kind: 'group'; type: string; count: number; totalBalance: number }
    | { kind: 'account'; node: AccountNode; depth: number };

  let rows = $derived.by(() => {
    const d = data;
    if (!d) return [];
    const result: TableRow[] = [];
    const isSearching = !!searchQuery;
    const q = searchQuery.toLowerCase();

    for (const type of Object.keys(d.byType)) {
      const info = d.byType[type];
      const accounts = flattenByType(d.trees, type);

      const filtered = isSearching
        ? accounts.filter(
            ({ node }) =>
              node.name.toLowerCase().includes(q) ||
              (node.code && node.code.toLowerCase().includes(q))
          )
        : accounts;

      if (filtered.length === 0 && !isSearching) continue;

      if (!isSearching) {
        result.push({ kind: 'group', type, count: info.count, totalBalance: info.totalBalance });
      }

      if (isSearching || expandedGroups.includes(type)) {
        for (const { node, depth } of filtered) {
          if (depth > 0 || isSearching) {
            result.push({ kind: 'account', node, depth });
          }
        }
      }
    }
    return result;
  });

  let typeOrder = $derived(Object.keys(data?.byType ?? {}));

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

    fetchAccounts({})
      .then((result) => {
        if (cancelled) return;
        data = result;
        if (!initialExpandDone) {
          expanded = [...allNodeGuids];
          expandedGroups = [...typeOrder];
          initialExpandDone = true;
        }
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

  $effect(() => {
    if (!dbSession.value) goto('/');
  });
</script>

<svelte:head>
  <title>Accounts - Spendo</title>
</svelte:head>

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

    <Card.Root class="pt-0">
      <Card.Content class="p-0">
        <Table.Root class="border-collapse">
          <Table.Header>
            <Table.Row>
              <Table.Head class="px-4 py-3 text-muted-foreground">Account</Table.Head>
              <Table.Head class="px-4 py-3 text-right text-muted-foreground">Balance</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each Array(8) as _, i (i)}
              <Table.Row>
                <Table.Cell><Skeleton class="h-4 w-48" /></Table.Cell>
                <Table.Cell class="text-right"><Skeleton class="ml-auto h-4 w-20" /></Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </Table.Root>
      </Card.Content>
    </Card.Root>
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
      {#if hasExpandableNodes && !searchQuery}
        <Button
          variant="outline"
          onclick={expandAll}
          disabled={expanded.length === allNodeGuids.length}
        >
          Expand all
        </Button>
        <Button variant="outline" onclick={collapseAll} disabled={expanded.length === 0}>
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

    <!-- Treetable -->
    <Card.Root class="pt-0">
      <Card.Content class="p-0">
        <Table.Root class="border-collapse">
          <Table.Header>
            <Table.Row class="bg-muted/50">
              <Table.Head class="w-full px-4 py-3 text-muted-foreground">Account</Table.Head>
              <Table.Head class="px-4 py-3 text-right text-muted-foreground">Balance</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each rows as row (row.kind === 'group' ? row.type : row.node.guid)}
              {#if row.kind === 'group'}
                <Table.Row class="bg-muted/30 cursor-pointer" onclick={() => toggleGroup(row.type)}>
                  <Table.Cell colspan={2} class="p-0">
                    <div class="flex items-center gap-2 px-3 py-2">
                      <ChevronRight
                        aria-hidden="true"
                        class="size-4 text-muted-foreground transition-transform {expandedGroups.includes(
                          row.type
                        )
                          ? 'rotate-90'
                          : ''}"
                      />
                      <span class="text-sm font-semibold">{row.type}</span>
                      <Badge variant="secondary" class="text-[10px]">{fmtCount(row.count)}</Badge>
                      <span
                        class="ml-auto tabular-nums text-sm font-medium {balanceColor(
                          row.totalBalance,
                          row.type
                        )}"
                      >
                        {row.totalBalance >= 0 ? '+' : ''}{fmt(row.totalBalance)}
                      </span>
                    </div>
                  </Table.Cell>
                </Table.Row>
              {:else}
                <Table.Row class="hover:bg-muted/50">
                  <Table.Cell class="p-0">
                    <div
                      class="flex items-center gap-1 px-3 py-2"
                      style="padding-left: {0.75 + row.depth * 1.25}rem"
                    >
                      {#if row.node.children.length > 0}
                        <button
                          type="button"
                          class="flex size-4 shrink-0 items-center justify-center rounded hover:bg-muted-foreground/10"
                          onclick={() => toggle(row.node.guid)}
                        >
                          <ChevronRight
                            aria-hidden="true"
                            class="size-3 text-muted-foreground transition-transform {expanded.includes(
                              row.node.guid
                            )
                              ? 'rotate-90'
                              : ''}"
                          />
                        </button>
                      {:else}
                        <div class="size-4 shrink-0"></div>
                      {/if}
                      <a
                        href={resolve(`/accounts/${row.node.guid}`)}
                        class="min-w-0 truncate text-sm font-medium hover:underline"
                      >
                        {row.node.name}
                      </a>
                      {#if row.node.code}
                        <span class="hidden sm:inline text-xs text-muted-foreground"
                          >{row.node.code}</span
                        >
                      {/if}
                      {#if row.node.placeholder}
                        <Badge
                          variant="outline"
                          class="shrink-0 text-[10px] leading-none text-muted-foreground"
                        >
                          PL
                        </Badge>
                      {/if}
                    </div>
                  </Table.Cell>
                  <Table.Cell
                    class="text-right tabular-nums text-sm {balanceColor(
                      row.node.balance,
                      nodeCategory(row.node)
                    )}"
                  >
                    {fmt(row.node.balance)}
                  </Table.Cell>
                </Table.Row>
              {/if}
            {/each}
          </Table.Body>
        </Table.Root>
      </Card.Content>
    </Card.Root>
  {/if}
</div>
