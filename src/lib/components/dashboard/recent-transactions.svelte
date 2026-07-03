<script lang="ts">
  import { ArrowLeftRight, TrendingUp, TrendingDown } from '@lucide/svelte';
  import * as Card from '$lib/components/ui/card';
  import { formatDate as fmtDate } from '$lib/format.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { resolve } from '$app/paths';
  import type { DashboardTransaction } from '$lib/server/dashboard-data';

  let { data }: { data: DashboardTransaction[] } = $props();

  function fmt(n: number): string {
    return n.toLocaleString(appSettings.value.locale, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    });
  }

  type TxKind = 'income' | 'expense' | 'transfer';

  function kind(tx: DashboardTransaction): TxKind {
    if (tx.accountType === 'INCOME') return 'income';
    if (tx.accountType === 'EXPENSE') return 'expense';
    return 'transfer';
  }
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Recent Transactions</Card.Title>
    <Card.Description>Last 10 transactions</Card.Description>
  </Card.Header>
  <Card.Content>
    {#if data.length === 0}
      <p class="py-4 text-center text-sm text-muted-foreground">No transactions found</p>
    {:else}
      <div class="flex flex-col gap-1">
        {#each data as tx (tx.guid)}
          {@const txKind = kind(tx)}
          <div
            class="flex items-center gap-3 rounded-md px-2 py-2 transition-colors hover:bg-muted/50"
          >
            <div class="flex size-8 shrink-0 items-center justify-center rounded-full bg-muted">
              {#if txKind === 'expense'}
                <TrendingDown aria-hidden="true" class="size-4 text-red-600 dark:text-red-400" />
              {:else if txKind === 'income'}
                <TrendingUp
                  aria-hidden="true"
                  class="size-4 text-emerald-600 dark:text-emerald-400"
                />
              {:else}
                <ArrowLeftRight
                  aria-hidden="true"
                  class="size-4 text-amber-600 dark:text-amber-400"
                />
              {/if}
            </div>
            <div class="flex min-w-0 flex-1 flex-col gap-0.5">
              <span class="truncate text-sm font-medium"
                >{tx.description || '(no description)'}</span
              >
              <span class="text-xs text-muted-foreground"
                >{tx.accountName} &middot; {fmtDate(tx.postDate, {
                  month: 'short',
                  day: 'numeric'
                })}</span
              >
            </div>
            <span
              class="shrink-0 text-sm font-medium tabular-nums {txKind === 'expense'
                ? 'text-red-600 dark:text-red-400'
                : txKind === 'income'
                  ? 'text-emerald-600 dark:text-emerald-400'
                  : 'text-amber-600 dark:text-amber-400'}"
            >
              {txKind === 'expense' ? '-' : txKind === 'income' ? '+' : ''}{fmt(tx.amount)}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </Card.Content>
  <Card.Footer>
    <a
      href={resolve('/transactions')}
      class="inline-flex items-center gap-1.5 text-sm font-medium text-muted-foreground underline underline-offset-2 decoration-muted-foreground/30 hover:text-foreground hover:decoration-foreground/50"
    >
      <ArrowLeftRight aria-hidden="true" class="size-4" />
      Browse all transactions
    </a>
  </Card.Footer>
</Card.Root>
