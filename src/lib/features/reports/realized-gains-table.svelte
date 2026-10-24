<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import Amount from '$lib/components/shared/amount.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import { openHolding } from '$lib/utils/navigation';
  import { formatDate, formatShares } from '$lib/utils/format.svelte';
  import type { RealizedGainRow } from '$lib/tauri';

  let {
    rows,
    totalProceeds,
    totalBasis,
    netGain
  }: {
    rows: RealizedGainRow[];
    totalProceeds: number;
    totalBasis: number;
    netGain: number;
  } = $props();
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Sold</Table.Head>
      <Table.Head class="text-muted-foreground">Holding</Table.Head>
      <Table.Head class="text-muted-foreground">Acquired</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Proceeds</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Cost basis</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Gain</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as r, i (`${r.date}:${r.commodityGuid}:${r.acquired}:${i}`)}
      <NavRow onNavigate={() => openHolding(r.commodityGuid)}>
        <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
          {formatDate(r.date)}
        </Table.Cell>
        <Table.Cell class="max-w-xs py-2.5">
          <a
            href={resolve('/portfolio/[holding]', { holding: r.commodityGuid })}
            class="flex flex-col gap-0.5 focus-visible:outline-2 focus-visible:outline-ring"
          >
            <span
              class="truncate font-medium"
              {@attach tooltip(r.fullName ?? r.mnemonic, { truncated: true })}
              >{r.fullName ?? r.mnemonic}</span
            >
            <span class="truncate text-xs text-muted-foreground">{r.mnemonic}</span>
          </a>
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
          {#if r.acquired}
            {formatDate(r.acquired)}
          {:else}
            <span {@attach tooltip('Average cost, or shares sold beyond what was held')}
              ><EmptyValue /></span
            >
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right tabular-nums">{formatShares(r.shares)}</Table.Cell>
        <Table.Cell class="py-2.5 text-right"><Amount value={r.proceeds} /></Table.Cell>
        <Table.Cell class="py-2.5 text-right"><Amount value={r.basis} /></Table.Cell>
        <Table.Cell class="py-2.5 text-right"><Amount value={r.gain} tone /></Table.Cell>
      </NavRow>
    {/each}
  </Table.Body>
  <Table.Footer>
    <Table.Row class="border-t bg-muted/50 font-semibold">
      <Table.Cell colspan={4} class="py-2.5 text-xs font-semibold tracking-wider uppercase"
        >Total</Table.Cell
      >
      <Table.Cell class="py-2.5 text-right"
        ><Amount value={totalProceeds} weight="semibold" /></Table.Cell
      >
      <Table.Cell class="py-2.5 text-right"
        ><Amount value={totalBasis} weight="semibold" /></Table.Cell
      >
      <Table.Cell class="py-2.5 text-right"
        ><Amount value={netGain} tone weight="semibold" /></Table.Cell
      >
    </Table.Row>
  </Table.Footer>
</TableShell>
