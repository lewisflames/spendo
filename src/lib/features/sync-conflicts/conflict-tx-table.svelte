<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import Amount from '$lib/components/shared/amount.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { formatDate } from '$lib/utils/format.svelte';
  import type { ConflictTx } from '$lib/tauri';

  /** Transactions one side has and the other doesn't. `linked` rows exist in
   * the open book, so they open its transaction page. */
  let { rows, linked }: { rows: ConflictTx[]; linked: boolean } = $props();
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Date</Table.Head>
      <Table.Head class="text-muted-foreground">Description</Table.Head>
      <Table.Head class="text-muted-foreground">Accounts</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Amount</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as tx (tx.guid)}
      <Table.Row>
        <Table.Cell class="py-2.5 whitespace-nowrap tabular-nums"
          >{formatDate(tx.postDate)}</Table.Cell
        >
        <Table.Cell class="max-w-xs truncate py-2.5 font-medium">
          {#if linked}
            <a class="hover:underline" href={resolve('/transactions/[guid]', { guid: tx.guid })}>
              {tx.description || 'Transaction'}
            </a>
          {:else}
            {tx.description || 'Transaction'}
          {/if}
          {#if tx.payee}
            <p class="text-xs font-normal text-muted-foreground">{tx.payee}</p>
          {/if}
        </Table.Cell>
        <Table.Cell
          class="max-w-xs truncate py-2.5 text-muted-foreground"
          {@attach tooltip(tx.accounts.join(', '), { truncated: true })}
        >
          {#if tx.accounts.length > 0}
            {tx.accounts.join(', ')}
          {:else}
            <EmptyValue />
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right"><Amount value={tx.amount} /></Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
