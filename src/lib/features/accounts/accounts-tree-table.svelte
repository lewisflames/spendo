<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import {
    formatNumber,
    formatSessionCurrency,
    balanceColor,
    clampZero
  } from '$lib/utils/format.svelte';
  import { nodeCategory, CATEGORY_LABEL_SINGULAR } from '$lib/features/accounts/account-types';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TreeChevron from '$lib/components/shared/tree-chevron.svelte';
  import { treeIndentStyle } from '$lib/utils';
  import { dbSession } from '$lib/stores/db.svelte';
  import type { AccountNode } from '$lib/tauri';
  import type { SvelteSet } from 'svelte/reactivity';

  let {
    rows,
    tree,
    searchActive
  }: {
    rows: { node: AccountNode; depth: number }[];
    tree: { expanded: SvelteSet<string>; toggle: (guid: string) => void };
    searchActive: boolean;
  } = $props();

  const reportCurrency = $derived(dbSession.value?.summary.currency ?? '');
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Account</Table.Head>
      <Table.Head class="text-muted-foreground">Type</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Txn</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as row (row.node.guid)}
      {@const node = row.node}
      <Table.Row
        class={row.depth === 0
          ? 'border-t border-b bg-muted/75 font-semibold'
          : node.children.length > 0
            ? 'bg-muted/50'
            : ''}
      >
        <Table.Cell class="p-0">
          <div
            class="flex items-center gap-1 py-2.5"
            style={row.depth === 0 ? undefined : treeIndentStyle(row.depth - 1)}
          >
            {#if row.depth > 0}
              <TreeChevron
                open={tree.expanded.has(node.guid)}
                interactive={node.children.length > 0 && !searchActive}
                ontoggle={() => tree.toggle(node.guid)}
              />
            {/if}
            <a
              href={resolve('/accounts/[guid]', { guid: node.guid })}
              class={row.depth === 0
                ? 'min-w-0 truncate text-xs font-semibold tracking-wider text-muted-foreground uppercase hover:underline'
                : 'min-w-0 truncate text-sm font-medium hover:underline'}
            >
              {node.description || node.name}
            </a>
            {#if node.code}
              <span class="hidden text-xs text-muted-foreground lg:inline">{node.code}</span>
            {/if}
            {#if node.commodity && node.commodity !== reportCurrency}
              <span
                class="rounded-sm border border-border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
                title={`Balance converted from ${node.commodity}`}
              >
                {node.commodity}
              </span>
            {/if}
          </div>
        </Table.Cell>
        <Table.Cell
          class={'text-sm text-muted-foreground ' + (row.depth === 0 ? 'py-2 ' : 'py-2.5 ')}
        >
          {CATEGORY_LABEL_SINGULAR[nodeCategory(node.accountType)] ?? node.accountType}
        </Table.Cell>
        <Table.Cell
          class={'text-right text-sm text-muted-foreground tabular-nums ' +
            (row.depth === 0 ? 'py-2 ' : 'py-2.5 ')}
        >
          {formatNumber(node.txCount)}
        </Table.Cell>
        <Table.Cell
          class={'text-right font-medium whitespace-nowrap tabular-nums ' +
            (row.depth === 0 ? 'py-2 ' : 'py-2.5 ') +
            balanceColor(node.balance, nodeCategory(node.accountType))}
        >
          {formatSessionCurrency(clampZero(node.balance))}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
