<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import Amount from '$lib/components/shared/amount.svelte';
  import * as Table from '$lib/components/ui/table';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SectionRow from '$lib/components/shared/section-row.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
  import { treeIndentStyle } from '$lib/utils';
  import type { TaxSection, TaxSummary } from '$lib/tauri';
  import { yearRange } from './tax-utils';

  let { summary }: { summary: TaxSummary } = $props();

  const COLUMNS = 2;

  /** Account ledger narrowed to the year the amount comes from. */
  function ledgerHref(guid: string, year: number): string {
    const { from, to } = yearRange(year);
    return `${resolve('/accounts/[guid]', { guid })}?from=${from}&to=${to}`;
  }

  let sections = $derived<{ label: string; section: TaxSection; year: number; empty: string }[]>([
    {
      label: 'Employment income',
      section: summary.employmentIncome,
      year: summary.year,
      empty: 'No wages recorded'
    },
    {
      label: 'Withholding on wages',
      section: summary.employmentWithholding,
      year: summary.year,
      empty: 'Nothing withheld'
    },
    {
      label: 'Social contributions',
      section: summary.socialContributions,
      year: summary.year,
      empty: 'None recorded'
    },
    {
      label: 'Savings income',
      section: summary.savingsIncome,
      year: summary.year,
      empty: 'No interest or dividends'
    },
    {
      label: 'Withholding on savings',
      section: summary.savingsWithholding,
      year: summary.year,
      empty: 'Nothing withheld'
    },
    {
      label: `Annual return, settled in ${summary.settlementYear}`,
      section: summary.settlement,
      year: summary.settlementYear,
      empty: summary.isPartial ? 'Not filed yet' : 'Nothing paid or refunded'
    }
  ]);
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-right text-muted-foreground">{summary.year}</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each sections as { label, section, year, empty } (label)}
      <SectionRow {label} total={section.total} columns={COLUMNS} />
      {#each section.lines as line (line.accountGuid)}
        {@const href = ledgerHref(line.accountGuid, year)}
        <!-- eslint-disable svelte/no-navigation-without-resolve -- href is resolved above -->
        <NavRow onNavigate={() => goto(href)}>
          <Table.Cell class="p-0">
            <div class="py-2.5" style={treeIndentStyle(1)}>
              <a
                {href}
                class="block min-w-0 text-sm focus-visible:outline-2 focus-visible:outline-ring"
              >
                <AccountLabel name={line.accountName} code={line.accountCode} />
              </a>
            </div>
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right"><Amount value={line.amount} /></Table.Cell>
        </NavRow>
        <!-- eslint-enable svelte/no-navigation-without-resolve -->
      {:else}
        <Table.Row class="hover:bg-transparent">
          <Table.Cell colspan={COLUMNS} class="py-2.5 text-sm text-muted-foreground">
            <span style={treeIndentStyle(1)} class="block">{empty}</span>
          </Table.Cell>
        </Table.Row>
      {/each}
    {/each}
    <SectionRow label="Income tax for {summary.year}" total={summary.incomeTax} columns={COLUMNS} />
  </Table.Body>
</TableShell>
