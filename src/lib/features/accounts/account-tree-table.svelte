<script lang="ts">
  import Amount from '$lib/components/shared/amount.svelte';
  import { resolve } from '$app/paths';
  import { ChevronRight } from '@lucide/svelte';
  import * as Table from '$lib/components/ui/table';
  import { Button } from '$lib/components/ui/button';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { clampZero, formatNumber, formatSessionCurrency } from '$lib/utils/format.svelte';
  import { openAccount } from '$lib/utils/navigation';
  import { cn, treeIndentStyle } from '$lib/utils';
  import AccountLabel from './account-label.svelte';
  import CommodityBadge from './commodity-badge.svelte';
  import { visibleAccountRows, type AccountTreeSection } from './account-tree';

  let {
    sections,
    expanded,
    query = '',
    reportCurrency,
    onToggle
  }: {
    sections: AccountTreeSection[];
    expanded: ReadonlySet<string>;
    query?: string;
    reportCurrency: string;
    onToggle: (guid: string) => void;
  } = $props();

  let rendered = $derived(
    sections
      .map((section) => ({ section, rows: visibleAccountRows(section.roots, expanded, query) }))
      .filter((entry) => entry.rows.length > 0)
  );

  /** `BANK` → `Bank`: raw account types read as shouting in a dense table. */
  function typeLabel(accountType: string): string {
    return accountType.charAt(0) + accountType.slice(1).toLowerCase();
  }
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-muted-foreground">Type</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Transactions</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rendered as { section, rows } (section.category)}
      <Table.Row class="hover:bg-transparent">
        <Table.Cell
          colspan={3}
          class="pt-4 pb-1 text-xs font-semibold tracking-wider text-muted-foreground uppercase"
        >
          {section.label}
        </Table.Cell>
        <Table.Cell
          class="pt-4 pb-1 text-right text-xs font-semibold whitespace-nowrap text-muted-foreground tabular-nums"
        >
          {formatSessionCurrency(clampZero(section.total))}
        </Table.Cell>
      </Table.Row>
      {#each rows as row (row.node.guid)}
        <NavRow onNavigate={() => openAccount(row.node.guid)}>
          <Table.Cell class="p-0">
            <div class="flex items-center gap-1 py-1.5" style={treeIndentStyle(row.depth * 1.25)}>
              {#if row.hasChildren}
                <Button
                  variant="ghost"
                  size="icon-xs"
                  class="text-muted-foreground"
                  aria-label={row.expanded
                    ? `Collapse ${row.node.name}`
                    : `Expand ${row.node.name}`}
                  aria-expanded={row.expanded}
                  disabled={!!query}
                  onclick={() => onToggle(row.node.guid)}
                >
                  <ChevronRight
                    aria-hidden="true"
                    class={cn('transition-transform duration-150', row.expanded && 'rotate-90')}
                  />
                </Button>
              {:else}
                <span class="size-6 shrink-0" aria-hidden="true"></span>
              {/if}
              <a
                href={resolve('/accounts/[guid]', { guid: row.node.guid })}
                class="flex min-w-0 items-center text-sm focus-visible:outline-2 focus-visible:outline-ring"
              >
                <AccountLabel
                  name={row.node.name}
                  description={row.node.description}
                  code={row.node.code}
                />
                <CommodityBadge commodity={row.node.commodity} {reportCurrency} />
              </a>
            </div>
          </Table.Cell>
          <Table.Cell class="py-1.5 text-sm text-muted-foreground">
            {typeLabel(row.node.accountType)}
          </Table.Cell>
          <Table.Cell class="py-1.5 text-right text-sm text-muted-foreground tabular-nums">
            {formatNumber(row.node.txCount)}
          </Table.Cell>
          <Table.Cell class="py-1.5 text-right">
            <Amount value={row.total} weight={row.hasChildren ? 'semibold' : 'medium'} />
          </Table.Cell>
        </NavRow>
      {/each}
    {/each}
  </Table.Body>
</TableShell>
