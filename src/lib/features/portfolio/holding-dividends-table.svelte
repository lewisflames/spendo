<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import { HandCoins } from '@lucide/svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import Amount from '$lib/components/shared/amount.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import { useResetPageOnChange } from '$lib/hooks/use-page-reset.svelte';
  import { openTransaction } from '$lib/utils/navigation';
  import { formatDate } from '$lib/utils/format.svelte';
  import type { HoldingDividend } from '$lib/tauri';

  /** Newest first, in the report currency. */
  let { dividends }: { dividends: HoldingDividend[] } = $props();

  let total = $derived(dividends.length);
  let pageSize = $derived(appSettings.value.pageSize);
  let totalPages = $derived(Math.max(1, Math.ceil(total / pageSize)));
  let page = $state(1);

  // Clamp back into range when the list shrinks or the page size grows.
  $effect(() => {
    if (page > totalPages) page = totalPages;
  });
  useResetPageOnChange(
    () => dividends,
    () => {
      page = 1;
    }
  );

  let pageRows = $derived(dividends.slice((page - 1) * pageSize, page * pageSize));
</script>

{#if total === 0}
  <EmptyState
    card
    title="No dividends yet"
    description="Dividends show up here when the payment touches this holding’s account, or comes from an income account you link to it."
  >
    {#snippet icon()}
      <HandCoins aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <div class="flex flex-col gap-4">
    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="text-muted-foreground">Date</Table.Head>
          <Table.Head class="text-muted-foreground">Description</Table.Head>
          <Table.Head class="text-right text-muted-foreground">Amount</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each pageRows as d (d.txGuid)}
          <NavRow onNavigate={() => openTransaction(d.txGuid)}>
            <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {formatDate(d.postDate)}
            </Table.Cell>
            <Table.Cell class="max-w-md py-2.5 font-medium">
              <a
                class="block truncate focus-visible:outline-2 focus-visible:outline-ring"
                href={resolve('/transactions/[guid]', { guid: d.txGuid })}
                {@attach tooltip(d.description ?? undefined, { truncated: true })}
              >
                {#if d.description}{d.description}{:else}<EmptyValue />{/if}
              </a>
            </Table.Cell>
            <Table.Cell class="py-2.5 text-right">
              <Amount value={d.amount} tone />
            </Table.Cell>
          </NavRow>
        {/each}
      </Table.Body>
    </TableShell>
    <PaginationNav
      bind:page
      {totalPages}
      totalItems={total}
      {pageSize}
      onPageSizeChange={(size) => applyPageSize(size, (v) => (page = v))}
    />
  </div>
{/if}
