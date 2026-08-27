<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { formatNumber, formatSessionCurrency, clampZero } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { handleRowClick, openAccount } from '$lib/utils/navigation';
  import { dbSession } from '$lib/stores/db.svelte';
  import type { AccountNode } from '$lib/tauri';

  let { accounts = [] }: { accounts?: AccountNode[] } = $props();

  const reportCurrency = $derived(dbSession.value?.summary.currency ?? '');
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Txn</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each accounts as node (node.guid)}
      <Table.Row
        class="cursor-pointer hover:bg-muted/50"
        onclick={(e) => handleRowClick(e, () => openAccount(node.guid))}
      >
        <Table.Cell class="p-0">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. Its event must keep bubbling so
               SvelteKit's router can intercept it. -->
          <a
            class="block min-w-0"
            href={resolve('/accounts/[guid]', { guid: node.guid })}
            title={node.description ?? undefined}
          >
            <span class="block min-w-0 truncate font-medium">{node.description || node.name}</span>
            {#if node.code}
              <span class="block truncate text-xs text-muted-foreground">{node.code}</span>
            {/if}
          </a>
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right text-sm text-muted-foreground tabular-nums">
          {formatNumber(node.txCount)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
          {formatSessionCurrency(clampZero(node.balance))}
          {#if node.commodity && node.commodity !== reportCurrency}
            <span
              class="ml-1 rounded-sm border border-border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
              title={`Balance converted from ${node.commodity}`}
            >
              {node.commodity}
            </span>
          {/if}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
