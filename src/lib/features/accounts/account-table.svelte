<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { formatNumber, formatSessionCurrency, clampZero } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { handleRowClick, openAccount } from '$lib/utils/navigation';
  import { dbSession } from '$lib/stores/db.svelte';
  import type { AccountNode, ChildAccountInfo } from '$lib/tauri';
  import CommodityBadge from './commodity-badge.svelte';

  type Row = Pick<
    AccountNode,
    'guid' | 'name' | 'description' | 'code' | 'commodity' | 'accountType' | 'balance' | 'txCount'
  > &
    Partial<Pick<ChildAccountInfo, 'guid'>>;

  let {
    rows = [],
    showType = false,
    nameHeader = 'Account'
  }: {
    rows?: Row[];
    showType?: boolean;
    nameHeader?: string;
  } = $props();

  const reportCurrency = $derived(dbSession.value?.summary.currency ?? '');
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">{nameHeader}</Table.Head>
      {#if showType}
        <Table.Head class="text-muted-foreground">Type</Table.Head>
      {/if}
      <Table.Head class="text-right text-muted-foreground">Txn</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as node (node.guid)}
      <Table.Row
        class="cursor-pointer hover:bg-muted/50"
        onclick={(e) => handleRowClick(e, () => openAccount(node.guid))}
      >
        <Table.Cell class="p-0">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. -->
          <a
            class="block min-w-0 px-2 py-2.5"
            href={resolve('/accounts/[guid]', { guid: node.guid })}
            title={node.description ?? undefined}
          >
            <span class="flex min-w-0 items-center gap-1">
              <span class="min-w-0 truncate font-medium">{node.description || node.name}</span>
              {#if node.code}
                <span class="hidden truncate text-xs text-muted-foreground lg:inline"
                  >{node.code}</span
                >
              {/if}
              <CommodityBadge commodity={node.commodity ?? null} {reportCurrency} />
            </span>
          </a>
        </Table.Cell>
        {#if showType}
          <Table.Cell class="py-2.5 text-sm text-muted-foreground">
            {node.accountType}
          </Table.Cell>
        {/if}
        <Table.Cell class="py-2.5 text-right text-sm text-muted-foreground tabular-nums">
          {formatNumber(node.txCount)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
          {formatSessionCurrency(clampZero(node.balance))}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
