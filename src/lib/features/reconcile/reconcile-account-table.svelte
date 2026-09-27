<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import {
    EMPTY_DASH,
    formatCurrency,
    formatDate,
    formatNumber,
    formatSessionCurrency,
    clampZero
  } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import { openReconcile } from '$lib/utils/navigation';
  import { dbSession } from '$lib/stores/db.svelte';
  import CommodityBadge from '$lib/features/accounts/commodity-badge.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
  import type { ReconcilableAccount } from './reconcile-accounts';

  let { rows = [] }: { rows?: ReconcilableAccount[] } = $props();

  const reportCurrency = $derived(dbSession.value?.summary.currency ?? '');
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="hidden text-muted-foreground sm:table-cell">Type</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Uncleared</Table.Head>
      <Table.Head class="hidden text-right text-muted-foreground sm:table-cell">Saved</Table.Head>
      <Table.Head class="hidden text-muted-foreground md:table-cell">Last reconciled</Table.Head>
      <Table.Head class="hidden text-right text-muted-foreground xl:table-cell">Txn</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as row (row.guid)}
      <NavRow onNavigate={() => openReconcile(row.guid)}>
        <Table.Cell class="py-2.5">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. Cell padding comes from TableShell
               (px-4) so all tables share the same horizontal rhythm. -->
          <a
            class="block min-w-0 focus-visible:outline-2 focus-visible:outline-ring"
            href={resolve('/reconcile/[guid]', { guid: row.guid })}
          >
            <span class="flex min-w-0 items-center gap-1">
              <AccountLabel name={row.name} description={row.description} code={row.code} />
              {#if row.parentPath}
                <span class="min-w-0 truncate text-xs text-muted-foreground"
                  >· {row.parentPath}</span
                >
              {/if}
              <CommodityBadge commodity={row.commodity ?? null} {reportCurrency} />
            </span>
          </a>
        </Table.Cell>
        <Table.Cell class="hidden py-2.5 text-sm text-muted-foreground sm:table-cell">
          {row.accountType}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right text-sm tabular-nums">
          {#if row.stats.unreconciledCount > 0}
            <span class="font-medium">{formatNumber(row.stats.unreconciledCount)}</span>
            <span class="block text-xs text-muted-foreground">
              {formatCurrency(row.stats.unclearedTotal, row.commodity ?? reportCurrency)}
            </span>
          {:else}
            <EmptyValue />
          {/if}
        </Table.Cell>
        <Table.Cell class="hidden py-2.5 text-right sm:table-cell">
          {#if row.stats.clearedCount > 0}
            <Badge variant="warning" title="Cleared marks saved but not finished" class="ml-auto">
              {formatNumber(row.stats.clearedCount)}
            </Badge>
          {:else}
            <EmptyValue />
          {/if}
        </Table.Cell>
        <Table.Cell class="hidden py-2.5 text-sm text-muted-foreground md:table-cell">
          {row.stats.lastReconciledDate ? formatDate(row.stats.lastReconciledDate) : EMPTY_DASH}
        </Table.Cell>
        <Table.Cell
          class="hidden py-2.5 text-right text-sm text-muted-foreground tabular-nums xl:table-cell"
        >
          {formatNumber(row.txCount)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
          {formatSessionCurrency(clampZero(row.balance))}
        </Table.Cell>
      </NavRow>
    {/each}
  </Table.Body>
</TableShell>
