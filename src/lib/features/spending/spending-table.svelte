<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Table from '$lib/components/ui/table';
  import { goto } from '$app/navigation';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import Amount from '$lib/components/shared/amount.svelte';
  import { fmtPct, formatAllocPct, signColor } from '$lib/utils/format.svelte';
  import { ChevronRight } from '@lucide/svelte';
  import type { SpendingCategory } from '$lib/tauri';
  import { categoryLabel, OTHER_COLOR, sharePct } from './spending-utils';

  let {
    categories,
    colors,
    total,
    totalPrev,
    changePct,
    hrefFor
  }: {
    categories: SpendingCategory[];
    /** Swatch per category GUID, matching the chart. */
    colors: Map<string, string>;
    total: number;
    totalPrev: number;
    changePct: number | null;
    /** Drill-down link for categories with children, the account ledger otherwise. */
    hrefFor: (c: SpendingCategory) => string;
  } = $props();
</script>

{#snippet change(pct: number | null)}
  <!-- More spending is the bad direction. -->
  <span
    class="tabular-nums {pct === null || pct === 0
      ? 'text-muted-foreground'
      : signColor(pct, false)}">{fmtPct(pct)}</span
  >
{/snippet}

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Category</Table.Head>
      <Table.Head class="w-40 text-muted-foreground">Share</Table.Head>
      <Table.Head class="text-right text-muted-foreground">This period</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Previous</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Change</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each categories as c (c.accountGuid + (c.ownSplits ? ':own' : ''))}
      {@const href = hrefFor(c)}
      {@const share = sharePct(c.total, total)}
      {@const color = colors.get(c.accountGuid) ?? OTHER_COLOR}
      <!-- Callers pass resolved routes with a query suffix. -->
      <!-- eslint-disable svelte/no-navigation-without-resolve -->
      <NavRow onNavigate={() => void goto(href)}>
        <Table.Cell class="max-w-xs py-2.5">
          <a
            {href}
            class="flex min-w-0 items-center gap-2 font-medium focus-visible:outline-2 focus-visible:outline-ring"
          >
            <span
              aria-hidden="true"
              class="size-2.5 shrink-0 rounded-[3px]"
              style:background-color={color}
            ></span>
            <span class="truncate" {@attach tooltip(categoryLabel(c), { truncated: true })}
              >{categoryLabel(c)}</span
            >
            {#if c.hasChildren}
              <ChevronRight aria-hidden="true" class="size-3.5 shrink-0 text-muted-foreground" />
            {/if}
          </a>
          <!-- eslint-enable svelte/no-navigation-without-resolve -->
        </Table.Cell>
        <Table.Cell class="py-2.5">
          {#if share !== null && share > 0}
            <div class="flex items-center gap-2">
              <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                <div
                  class="h-full rounded-full"
                  style:width="{Math.min(100, share)}%"
                  style:background-color={color}
                ></div>
              </div>
              <span class="w-12 text-right text-xs text-muted-foreground tabular-nums"
                >{formatAllocPct(share)}</span
              >
            </div>
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right"><Amount value={c.total} /></Table.Cell>
        <Table.Cell class="py-2.5 text-right text-muted-foreground"
          ><Amount value={c.previousTotal} /></Table.Cell
        >
        <Table.Cell class="py-2.5 text-right">{@render change(c.changePct)}</Table.Cell>
      </NavRow>
    {/each}
  </Table.Body>
  <Table.Footer>
    <Table.Row class="border-t bg-muted/50 font-semibold">
      <Table.Cell colspan={2} class="py-2.5 text-xs font-semibold tracking-wider uppercase"
        >Total</Table.Cell
      >
      <Table.Cell class="py-2.5 text-right"><Amount value={total} weight="semibold" /></Table.Cell>
      <Table.Cell class="py-2.5 text-right"
        ><Amount value={totalPrev} weight="semibold" /></Table.Cell
      >
      <Table.Cell class="py-2.5 text-right">{@render change(changePct)}</Table.Cell>
    </Table.Row>
  </Table.Footer>
</TableShell>
