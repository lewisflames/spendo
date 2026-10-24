<script lang="ts">
  import Amount from '$lib/components/shared/amount.svelte';
  import * as Table from '$lib/components/ui/table';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SectionRow from '$lib/components/shared/section-row.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import ReportAccountCell from '$lib/features/reports/report-account-cell.svelte';
  import { openAccount } from '$lib/utils/navigation';
  import type { BalanceSheet, BalanceSheetRow } from './balance-sheet';

  let { sheet }: { sheet: BalanceSheet } = $props();
</script>

{#snippet tableHead()}
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Amount</Table.Head>
    </Table.Row>
  </Table.Header>
{/snippet}

{#snippet accountRow(row: BalanceSheetRow)}
  <NavRow onNavigate={() => openAccount(row.accountGuid)}>
    <ReportAccountCell
      depth={row.depth}
      guid={row.accountGuid}
      name={row.name}
      description={row.description}
      code={row.code}
    />
    <Table.Cell class="py-2.5 text-right">
      <Amount value={row.amount} />
    </Table.Cell>
  </NavRow>
{/snippet}

{#snippet syntheticRow(row: BalanceSheetRow)}
  <Table.Row class="hover:bg-transparent">
    <ReportAccountCell
      depth={row.depth}
      guid=""
      name={row.name}
      description={row.description}
      code={row.code}
    />
    <Table.Cell class="py-2.5 text-right">
      <Amount value={row.amount} />
    </Table.Cell>
  </Table.Row>
{/snippet}

{#snippet emptyRow()}
  <Table.Row class="hover:bg-transparent">
    <Table.Cell colspan={2} class="py-4 text-sm text-muted-foreground">No accounts</Table.Cell>
  </Table.Row>
{/snippet}

{#snippet sectionRows(rows: BalanceSheetRow[])}
  {#if rows.length === 0}
    {@render emptyRow()}
  {:else}
    {#each rows as row (row.accountGuid || row.name)}
      {#if row.accountGuid}
        {@render accountRow(row)}
      {:else}
        {@render syntheticRow(row)}
      {/if}
    {/each}
  {/if}
{/snippet}

{#snippet subtotalRow(label: string, amount: number)}
  <Table.Row class="border-t hover:bg-transparent">
    <Table.Cell class="font-medium">{label}</Table.Cell>
    <Table.Cell class="text-right"><Amount value={amount} /></Table.Cell>
  </Table.Row>
{/snippet}

{#snippet totalRow(label: string, amount: number)}
  <Table.Row class="border-t hover:bg-transparent">
    <Table.Cell class="font-semibold">{label}</Table.Cell>
    <Table.Cell class="text-right"><Amount value={amount} weight="semibold" /></Table.Cell>
  </Table.Row>
{/snippet}

<!-- Two stacked statements become side by side on wide screens. Parent
     subtotals count their children, matching the P&L report rows; the footer
     totals come from the backend's per-category sums so they can't drift. -->
<div class="grid grid-cols-1 items-start gap-6 @5xl:grid-cols-2">
  <TableShell>
    {@render tableHead()}
    <Table.Body>
      <SectionRow label="Assets" columns={2} />
      {@render sectionRows(sheet.assets.rows)}
    </Table.Body>
    <Table.Footer>
      {@render totalRow('Total assets', sheet.assets.total)}
    </Table.Footer>
  </TableShell>

  <TableShell>
    {@render tableHead()}
    <Table.Body>
      <SectionRow label="Liabilities" columns={2} />
      {@render sectionRows(sheet.liabilities.rows)}
      {@render subtotalRow('Total liabilities', sheet.liabilities.total)}
      <SectionRow label="Equity" columns={2} />
      {@render sectionRows(sheet.equity.rows)}
      {@render subtotalRow('Total equity', sheet.equity.total)}
    </Table.Body>
    <Table.Footer>
      {@render totalRow('Total liabilities & equity', sheet.totalLiabilitiesAndEquity)}
    </Table.Footer>
  </TableShell>
</div>
