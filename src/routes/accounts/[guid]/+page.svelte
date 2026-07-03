<script lang="ts">
  import { page } from '$app/state';
  import * as Card from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import {
    ArrowLeft,
    ArrowRight,
    Database,
    TrendingUp,
    TrendingDown,
    Wallet
  } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { fetchAccountDetail } from '$lib/account-detail.remote';
  import { formatNumber, formatCurrency, formatDate as fmtDate } from '$lib/format.svelte';
  import EmptyState from '$lib/components/empty-state.svelte';
  import type { AccountDetailResult } from '$lib/server/account-detail';

  let guid = $derived(page.params.guid);
  let pageSize = $derived(appSettings.value.pageSize);
  let pg = $state(1);
  let data = $state<AccountDetailResult | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let currency = $derived(dbSession.value?.summary.currency ?? 'EUR');

  let currentBalance = $derived(data?.account.balance ?? 0);

  function fmt(n: number): string {
    return formatCurrency(n, currency);
  }

  let prevGuid: string | undefined = '';

  $effect(() => {
    if (!dbSession.value) {
      prevGuid = '';
      return;
    }

    const currentGuid = guid;
    if (currentGuid !== undefined && currentGuid !== prevGuid) {
      prevGuid = currentGuid;
      pg = 1;
    }
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

    fetchAccountDetail({
      token: dbSession.value.token as string,
      guid: guid as string,
      page: pg,
      pageSize
    })
      .then((result) => {
        if (cancelled) return;
        data = result;
        loading = false;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error = e instanceof Error ? e.message : 'Failed to load account detail';
        loading = false;
      });

    return () => {
      cancelled = true;
    };
  });

  function prevPage() {
    if (pg > 1) pg--;
  }

  function nextPage() {
    pg++;
  }
</script>

<svelte:head>
  <title>{data ? `${data.account.name} - Spendo` : 'Account - Spendo'}</title>
</svelte:head>

{#if !dbSession.value}
  <EmptyState title="No Data" description="Upload a GnuCash file to see account details.">
    {#snippet icon()}
      <Database aria-hidden="true" class="size-12 text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else if loading}
  <div class="flex flex-col gap-6">
    <div class="flex flex-col gap-1">
      <Skeleton class="h-8 w-48" />
      <Skeleton class="h-4 w-64" />
    </div>
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
      {#each Array(4) as _, i (i)}
        <Card.Root>
          <Card.Header>
            <Skeleton class="h-4 w-24" />
          </Card.Header>
          <Card.Content>
            <Skeleton class="h-7 w-28" />
          </Card.Content>
        </Card.Root>
      {/each}
    </div>
    <div class="overflow-x-auto rounded-lg border">
      <table class="w-full min-w-150">
        <thead>
          <tr class="bg-muted/50 text-left text-sm text-muted-foreground">
            <th class="px-4 py-3 font-medium">Date</th>
            <th class="px-4 py-3 font-medium">Description</th>
            <th class="px-4 py-3 font-medium">Splits</th>
            <th class="px-4 py-3 text-right font-medium">Running Balance</th>
          </tr>
        </thead>
        <tbody>
          {#each Array(5) as _, i (i)}
            <tr>
              <td class="px-4 py-3" colspan="4"><Skeleton class="h-5 w-full" /></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{:else if error}
  <Card.Root class="border-destructive">
    <Card.Content class="py-6 text-center text-sm text-destructive">
      {error}
    </Card.Content>
  </Card.Root>
{:else if data}
  {@const d = data}

  <div class="flex flex-col gap-6">
    <!-- Header -->
    <div class="flex flex-col gap-1">
      <h2 class="text-3xl font-bold tracking-tight">{d.account.name}</h2>
      <p class="text-sm text-muted-foreground">
        {d.account.accountType}
        {#if d.account.code}
          &middot; {d.account.code}{/if}
        {#if d.account.description}
          &middot; {d.account.description}{/if}
        {#if d.account.placeholder}
          &middot; Placeholder{/if}
        {#if d.account.childrenCount > 0}
          &middot; {d.account.childrenCount} sub-account{d.account.childrenCount === 1 ? '' : 's'}
        {/if}
      </p>
    </div>

    <!-- Summary cards -->
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
            <Card.Title>Total Balance</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p
            class="text-2xl font-bold tabular-nums {currentBalance >= 0
              ? 'text-emerald-600 dark:text-emerald-400'
              : 'text-red-600 dark:text-red-400'}"
          >
            {currentBalance >= 0 ? '+' : ''}{fmt(currentBalance)}
          </p>
        </Card.Content>
      </Card.Root>
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <Database aria-hidden="true" class="size-4 text-muted-foreground" />
            <Card.Title>Transactions</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p class="text-2xl font-bold">{formatNumber(d.summary.totalTransactions)}</p>
        </Card.Content>
      </Card.Root>
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <TrendingUp aria-hidden="true" class="size-4 text-emerald-600 dark:text-emerald-400" />
            <Card.Title>Income</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p class="text-2xl font-bold text-emerald-600 dark:text-emerald-400">
            +{fmt(d.summary.totalIncome)}
          </p>
        </Card.Content>
      </Card.Root>
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <TrendingDown aria-hidden="true" class="size-4 text-red-600 dark:text-red-400" />
            <Card.Title>Expenses</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p class="text-2xl font-bold text-red-600 dark:text-red-400">
            &minus;{fmt(d.summary.totalExpenses)}
          </p>
        </Card.Content>
      </Card.Root>
    </div>

    <!-- Transaction table -->
    {#if d.transactions.length === 0}
      <Card.Root>
        <Card.Content class="py-8 text-center text-sm text-muted-foreground">
          No transactions found for this account.
        </Card.Content>
      </Card.Root>
    {:else}
      <div class="overflow-x-auto rounded-lg border">
        <table class="w-full min-w-150">
          <thead>
            <tr class="bg-muted/50 text-left text-sm text-muted-foreground">
              <th class="px-4 py-3 font-medium">Date</th>
              <th class="px-4 py-3 font-medium">Description</th>
              <th class="px-4 py-3 font-medium">Splits</th>
              <th class="px-4 py-3 text-right font-medium">Running Balance</th>
            </tr>
          </thead>
          <tbody class="divide-y text-sm">
            {#each d.transactions as txn (txn.guid)}
              <tr class="transition-colors hover:bg-muted/30">
                <td class="whitespace-nowrap px-4 py-3 text-muted-foreground">
                  {fmtDate(txn.postDate)}
                </td>
                <td class="max-w-xs truncate px-4 py-3 font-medium">
                  {txn.description || '\u2014'}
                </td>
                <td class="px-4 py-3">
                  <div class="flex flex-col gap-1">
                    {#each txn.splits as split (split.guid)}
                      <div class="flex items-center gap-1.5">
                        <span class="min-w-0 truncate text-xs">{split.accountName}</span>
                        <Badge variant="outline" class="shrink-0 text-[10px] leading-none">
                          {split.accountType}
                        </Badge>
                        <span
                          class="ml-auto shrink-0 tabular-nums text-xs font-medium {split.amount
                            ? split.amount > 0
                              ? 'text-emerald-600 dark:text-emerald-400'
                              : 'text-red-600 dark:text-red-400'
                            : ''}"
                        >
                          {split.amount > 0 ? '+' : ''}{split.amount !== 0
                            ? fmt(Math.abs(split.amount))
                            : '0'}
                        </span>
                      </div>
                    {/each}
                  </div>
                </td>
                <td
                  class="whitespace-nowrap px-4 py-3 text-right font-medium tabular-nums {txn.runningBalance >=
                  0
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : 'text-red-600 dark:text-red-400'}"
                >
                  {fmt(txn.runningBalance)}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      {#if d.totalPages > 1}
        <div class="flex items-center justify-between">
          <p class="text-sm text-muted-foreground">
            Page {d.page} of {d.totalPages}
          </p>
          <div class="flex items-center gap-2">
            <Button variant="outline" size="sm" disabled={pg <= 1} onclick={prevPage}>
              <ArrowLeft aria-hidden="true" class="mr-1 size-3.5" />
              Previous
            </Button>
            <Button variant="outline" size="sm" disabled={pg >= d.totalPages} onclick={nextPage}>
              Next
              <ArrowRight aria-hidden="true" class="ml-1 size-3.5" />
            </Button>
          </div>
        </div>
      {/if}
    {/if}
  </div>
{/if}
