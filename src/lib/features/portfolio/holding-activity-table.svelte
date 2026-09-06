<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import { Download } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { signColor, formatSignedCurrency, formatDate } from '$lib/utils/format.svelte';
  import { handleRowClick, openTransaction } from '$lib/utils/navigation';
  import type { HoldingTransaction } from '$lib/tauri';

  let {
    transactions,
    currency,
    onExport
  }: {
    transactions: HoldingTransaction[];
    currency: string;
    onExport: () => void;
  } = $props();
</script>

<!-- Buy/Sell activity -->
{#if transactions.length > 0}
  <div>
    <div class="mb-2 flex items-center justify-between">
      <h2 class="text-sm font-medium text-foreground">Buy / Sell Activity</h2>
      <Button variant="ghost" size="sm" onclick={onExport}>
        <Download aria-hidden="true" class="size-3" />
        Export
      </Button>
    </div>
    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="text-muted-foreground">Date</Table.Head>
          <Table.Head class="text-muted-foreground">Description</Table.Head>
          <Table.Head class="text-muted-foreground">Account</Table.Head>
          <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
          <Table.Head class="text-right text-muted-foreground">Value</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each transactions as t (t.guid)}
          <Table.Row
            class="cursor-pointer hover:bg-muted/50"
            onclick={(e) => handleRowClick(e, () => openTransaction(t.guid))}
          >
            <Table.Cell class="py-2 text-muted-foreground tabular-nums">
              {formatDate(t.postDate)}
            </Table.Cell>
            <Table.Cell class="max-w-64 truncate py-2">
              <a href={resolve('/transactions/[id]', { id: t.guid })} class="block truncate">
                {t.description ?? '—'}
              </a>
            </Table.Cell>
            <Table.Cell class="py-2 text-xs text-muted-foreground">{t.accountName}</Table.Cell>
            <Table.Cell class="py-2 text-right tabular-nums {signColor(t.sharesDelta)}">
              {t.sharesDelta >= 0 ? '+' : ''}{t.sharesDelta.toFixed(2)}
            </Table.Cell>
            <Table.Cell class="py-2 text-right tabular-nums {signColor(t.sharesDelta)}">
              {formatSignedCurrency(t.valueDelta, currency)}
            </Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </TableShell>
  </div>
{/if}
