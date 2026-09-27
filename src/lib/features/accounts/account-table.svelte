<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { formatNumber, formatSessionCurrency, clampZero } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openAccount } from '$lib/utils/navigation';
  import { dbSession } from '$lib/stores/db.svelte';
  import type { AccountNode, ChildAccountInfo } from '$lib/tauri';
  import CommodityBadge from './commodity-badge.svelte';
  import AccountLabel from './account-label.svelte';

  type Row = Pick<
    AccountNode,
    'guid' | 'name' | 'description' | 'code' | 'commodity' | 'accountType' | 'balance' | 'txCount'
  > &
    Partial<Pick<ChildAccountInfo, 'guid'>>;

  let {
    rows = [],
    showType = false,
    nameHeader = 'Account'
  }: {
    rows?: Row[];
    showType?: boolean;
    nameHeader?: string;
  } = $props();

  const reportCurrency = $derived(dbSession.value?.summary.currency ?? '');
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">{nameHeader}</Table.Head>
      {#if showType}
        <Table.Head class="text-muted-foreground">Type</Table.Head>
      {/if}
      <Table.Head class="text-right text-muted-foreground">Txn</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as node (node.guid)}
      <NavRow onNavigate={() => openAccount(node.guid)}>
        <Table.Cell class="py-2.5">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. Cell padding comes from TableShell
               (px-4) so all tables share the same horizontal rhythm. -->
          <a
            class="block min-w-0 focus-visible:outline-2 focus-visible:outline-ring"
            href={resolve('/accounts/[guid]', { guid: node.guid })}
          >
            <span class="flex min-w-0 items-center gap-1">
              <AccountLabel name={node.name} description={node.description} code={node.code} />
              <CommodityBadge commodity={node.commodity ?? null} {reportCurrency} />
            </span>
          </a>
        </Table.Cell>
        {#if showType}
          <Table.Cell class="py-2.5 text-sm text-muted-foreground">
            {node.accountType}
          </Table.Cell>
        {/if}
        <Table.Cell class="py-2.5 text-right text-sm text-muted-foreground tabular-nums">
          {formatNumber(node.txCount)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
          {formatSessionCurrency(clampZero(node.balance))}
        </Table.Cell>
      </NavRow>
    {/each}
  </Table.Body>
</TableShell>
