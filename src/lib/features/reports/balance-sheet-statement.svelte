<script lang="ts">
  import Amount from '$lib/components/shared/amount.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import * as Table from '$lib/components/ui/table';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SectionRow from '$lib/components/shared/section-row.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import ReportAccountCell from '$lib/features/reports/report-account-cell.svelte';
  import { openAccount } from '$lib/utils/navigation';
  import { formatDate, formatSignedSessionCurrency } from '$lib/utils/format.svelte';
  import type { BalanceSheet, BalanceSheetRow } from './balance-sheet';

  let {
    sheet,
    asOf,
    compareTo = null
  }: {
    sheet: BalanceSheet;
    /** Valuation date (`YYYY-MM-DD`), labels the amount column. */
    asOf: string;
    /** Comparison date; adds comparison and change columns when set. */
    compareTo?: string | null;
  } = $props();

  let comparing = $derived(compareTo !== null && sheet.assets.compareTotal !== null);
  let columns = $derived(comparing ? 4 : 2);
</script>

{#snippet tableHead()}
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-right whitespace-nowrap text-muted-foreground">
        {formatDate(asOf)}
      </Table.Head>
      {#if comparing && compareTo}
        <Table.Head class="text-right whitespace-nowrap text-muted-foreground">
          {formatDate(compareTo)}
        </Table.Head>
        <Table.Head class="text-right whitespace-nowrap text-muted-foreground">Change</Table.Head>
      {/if}
    </Table.Row>
  </Table.Header>
{/snippet}

<!-- Comparison and change cells; `positiveGood` is false for liabilities,
     where growth is the bad direction. -->
{#snippet compareCells(
  amount: number,
  compareAmount: number | null,
  positiveGood: boolean,
  weight: 'medium' | 'semibold' = 'medium'
)}
  {#if comparing}
    {@const change = amount - (compareAmount ?? 0)}
    <Table.Cell class="py-2.5 text-right text-muted-foreground">
      <Amount value={compareAmount ?? 0} weight="normal" />
    </Table.Cell>
    <Table.Cell class="py-2.5 text-right">
      {#if Math.abs(change) < 0.005}
        <EmptyValue />
      {:else}
        <Amount value={change} format={formatSignedSessionCurrency} tone {positiveGood} {weight} />
      {/if}
    </Table.Cell>
  {/if}
{/snippet}

{#snippet accountRow(row: BalanceSheetRow, positiveGood: boolean)}
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
    {@render compareCells(row.amount, row.compareAmount, positiveGood)}
  </NavRow>
{/snippet}

{#snippet syntheticRow(row: BalanceSheetRow, positiveGood: boolean)}
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
    {@render compareCells(row.amount, row.compareAmount, positiveGood)}
  </Table.Row>
{/snippet}

{#snippet emptyRow()}
  <Table.Row class="hover:bg-transparent">
    <Table.Cell colspan={columns} class="py-4 text-sm text-muted-foreground">No accounts</Table.Cell
    >
  </Table.Row>
{/snippet}

{#snippet sectionRows(rows: BalanceSheetRow[], positiveGood: boolean)}
  {#if rows.length === 0}
    {@render emptyRow()}
  {:else}
    {#each rows as row (row.accountGuid || row.name)}
      {#if row.accountGuid}
        {@render accountRow(row, positiveGood)}
      {:else}
        {@render syntheticRow(row, positiveGood)}
      {/if}
    {/each}
  {/if}
{/snippet}

{#snippet subtotalRow(
  label: string,
  amount: number,
  compareAmount: number | null,
  positiveGood: boolean
)}
  <Table.Row class="border-t hover:bg-transparent">
    <Table.Cell class="font-medium">{label}</Table.Cell>
    <Table.Cell class="text-right"><Amount value={amount} /></Table.Cell>
    {@render compareCells(amount, compareAmount, positiveGood)}
  </Table.Row>
{/snippet}

{#snippet totalRow(
  label: string,
  amount: number,
  compareAmount: number | null,
  positiveGood: boolean
)}
  <Table.Row class="border-t hover:bg-transparent">
    <Table.Cell class="font-semibold">{label}</Table.Cell>
    <Table.Cell class="text-right"><Amount value={amount} weight="semibold" /></Table.Cell>
    {@render compareCells(amount, compareAmount, positiveGood, 'semibold')}
  </Table.Row>
{/snippet}

<!-- Two stacked statements become side by side on wide screens. Parent
     subtotals count their children, matching the P&L report rows; the footer
     totals come from the backend's per-category sums so they can't drift. -->
<div class="grid grid-cols-1 items-start gap-6 @5xl:grid-cols-2">
  <TableShell>
    {@render tableHead()}
    <Table.Body>
      <SectionRow label="Assets" {columns} />
      {@render sectionRows(sheet.assets.rows, true)}
    </Table.Body>
    <Table.Footer>
      {@render totalRow('Total assets', sheet.assets.total, sheet.assets.compareTotal, true)}
    </Table.Footer>
  </TableShell>

  <TableShell>
    {@render tableHead()}
    <Table.Body>
      <SectionRow label="Liabilities" {columns} />
      {@render sectionRows(sheet.liabilities.rows, false)}
      {@render subtotalRow(
        'Total liabilities',
        sheet.liabilities.total,
        sheet.liabilities.compareTotal,
        false
      )}
      <SectionRow label="Equity" {columns} />
      {@render sectionRows(sheet.equity.rows, true)}
      {@render subtotalRow('Total equity', sheet.equity.total, sheet.equity.compareTotal, true)}
    </Table.Body>
    <Table.Footer>
      {@render totalRow(
        'Total liabilities & equity',
        sheet.totalLiabilitiesAndEquity,
        sheet.compareTotalLiabilitiesAndEquity,
        true
      )}
    </Table.Footer>
  </TableShell>
</div>
