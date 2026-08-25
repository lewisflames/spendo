<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import {
    formatNumber,
    formatSessionCurrency,
    balanceColor,
    clampZero
  } from '$lib/utils/format.svelte';
  import { nodeCategory } from '$lib/features/accounts/account-types';
  import { CATEGORY_LABEL_SINGULAR } from '$lib/features/accounts/account-types';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import type { ChildAccountInfo } from '$lib/tauri';
  import { handleRowClick, openAccount } from '$lib/utils/navigation';

  let { nodes }: { nodes: ChildAccountInfo[] } = $props();

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
    {#each nodes as node (node.guid)}
      <Table.Row
        class="cursor-pointer hover:bg-muted/50"
        onclick={(e) => handleRowClick(e, () => openAccount(node.guid))}
      >
        <Table.Cell class="p-0">
          <div class="flex items-center gap-1 py-2.5">
            <a
              href={resolve('/accounts/[guid]', { guid: node.guid })}
              class="min-w-0 truncate text-sm font-medium"
            >
              {node.description || node.name}
            </a>
            {#if node.code}
              <span class="hidden text-xs text-muted-foreground lg:inline">{node.code}</span>
            {/if}
            {#if node.commodity && node.commodity !== reportCurrency}
              <span
                class="rounded-sm border border-border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
                title={`Balance converted from ${node.commodity}`}
              >
                {node.commodity}
              </span>
            {/if}
          </div>
        </Table.Cell>
        <Table.Cell class="py-2.5 text-sm text-muted-foreground">
          {CATEGORY_LABEL_SINGULAR[nodeCategory(node.accountType)] ?? node.accountType}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right text-sm text-muted-foreground tabular-nums">
          {formatNumber(node.txCount)}
        </Table.Cell>
        <Table.Cell
          class={'py-2.5 text-right font-medium whitespace-nowrap tabular-nums ' +
            balanceColor(node.balance, nodeCategory(node.accountType))}
        >
          {formatSessionCurrency(clampZero(node.balance))}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
