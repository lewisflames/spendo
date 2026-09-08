<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { formatNumber, formatSessionCurrency, clampZero } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openReconcile } from '$lib/utils/navigation';
  import { dbSession } from '$lib/stores/db.svelte';
  import CommodityBadge from '$lib/features/accounts/commodity-badge.svelte';
  import type { ReconcilableAccount } from './reconcile-accounts';

  let { rows = [] }: { rows?: ReconcilableAccount[] } = $props();

  const reportCurrency = $derived(dbSession.value?.summary.currency ?? '');
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-muted-foreground">Type</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Txn</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as row (row.guid)}
      <NavRow onNavigate={() => openReconcile(row.guid)}>
        <Table.Cell class="p-0">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. -->
          <a
            class="block min-w-0 px-2 py-2.5"
            href={resolve('/reconcile/[guid]', { guid: row.guid })}
            title={row.label}
          >
            <span class="flex min-w-0 items-center gap-1">
              <span class="min-w-0 truncate font-medium">{row.label}</span>
              {#if row.code}
                <span class="hidden truncate text-xs text-muted-foreground lg:inline"
                  >{row.code}</span
                >
              {/if}
              <CommodityBadge commodity={row.commodity ?? null} {reportCurrency} />
            </span>
          </a>
        </Table.Cell>
        <Table.Cell class="py-2.5 text-sm text-muted-foreground">
          {row.accountType}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right text-sm text-muted-foreground tabular-nums">
          {formatNumber(row.txCount)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
          {formatSessionCurrency(clampZero(row.balance))}
        </Table.Cell>
      </NavRow>
    {/each}
  </Table.Body>
</TableShell>
