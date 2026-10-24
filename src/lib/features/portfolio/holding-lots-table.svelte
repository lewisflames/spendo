<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Layers } from '@lucide/svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import Amount from '$lib/components/shared/amount.svelte';
  import { formatDate, formatShares } from '$lib/utils/format.svelte';
  import type { OpenLot } from '$lib/tauri';

  /** Oldest first, in the report currency. */
  let { lots }: { lots: OpenLot[] } = $props();
</script>

{#if lots.length === 0}
  <EmptyState card title="No open lots" description="Every share bought has been sold.">
    {#snippet icon()}
      <Layers aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <Table.Head class="text-muted-foreground">Acquired</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Cost per share</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Cost basis</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Market value</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Unrealized</Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each lots as lot, i (`${lot.acquired}:${i}`)}
        <Table.Row>
          <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
            {formatDate(lot.acquired)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">{formatShares(lot.shares)}</Table.Cell>
          <Table.Cell class="py-2.5 text-right">
            <Amount value={lot.basis / lot.shares} weight="normal" />
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right"><Amount value={lot.basis} /></Table.Cell>
          <Table.Cell class="py-2.5 text-right">
            {#if lot.marketValue !== null}<Amount value={lot.marketValue} />{:else}<EmptyValue
              />{/if}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right">
            {#if lot.unrealizedGain !== null}<Amount
                value={lot.unrealizedGain}
                tone
              />{:else}<EmptyValue />{/if}
          </Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
