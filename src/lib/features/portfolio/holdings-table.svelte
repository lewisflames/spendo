<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import { TrendingUp } from '@lucide/svelte';
  import type { PortfolioHolding } from '$lib/tauri';
  import {
    formatDate,
    signColor,
    fmtPct,
    formatSignedCurrency,
    formatSessionCurrency
  } from '$lib/utils/format.svelte';

  type SortKey =
    | 'name'
    | 'ticker'
    | 'shares'
    | 'avgCost'
    | 'price'
    | 'change'
    | 'marketValue'
    | 'costBasis'
    | 'gainLoss'
    | 'alloc';

  let {
    holdings = [] as PortfolioHolding[],
    currency,
    loading = false
  }: {
    holdings: PortfolioHolding[];
    currency: string;
    loading?: boolean;
  } = $props();

  let sortBy = $state<SortKey | null>(null);
  let sortDir = $state<'asc' | 'desc'>('desc');

  function dayChangePct(h: PortfolioHolding): number | null {
    if (h.price === null || h.prevPrice === null || h.prevPrice === 0) return null;
    return ((h.price - h.prevPrice) / h.prevPrice) * 100;
  }

  let sortedHoldings = $derived.by(() => {
    if (!sortBy) return holdings;
    const dir = sortDir === 'asc' ? 1 : -1;
    return [...holdings].sort((a, b) => {
      // Nulls (missing data) always sort to the bottom.
      const nullCmp = (va: number | null, vb: number | null) =>
        va === null && vb === null ? 0 : va === null ? 1 : vb === null ? -1 : 0;
      let cmp = 0;
      switch (sortBy) {
        case 'name':
          cmp = (a.fullName ?? a.mnemonic).localeCompare(b.fullName ?? b.mnemonic);
          break;
        case 'ticker':
          cmp = a.mnemonic.localeCompare(b.mnemonic);
          break;
        case 'shares':
          cmp = a.shares - b.shares;
          break;
        case 'avgCost':
          cmp = a.avgCostPerShare - b.avgCostPerShare;
          break;
        case 'price':
          cmp = nullCmp(a.price, b.price) || (a.price ?? 0) - (b.price ?? 0);
          break;
        case 'change': {
          const ca = dayChangePct(a);
          const cb = dayChangePct(b);
          cmp = nullCmp(ca, cb) || (ca ?? 0) - (cb ?? 0);
          break;
        }
        case 'marketValue':
          cmp = a.marketValue - b.marketValue;
          break;
        case 'costBasis':
          cmp = a.costBasis - b.costBasis;
          break;
        case 'gainLoss':
          cmp = a.gainLoss - b.gainLoss;
          break;
        case 'alloc':
          cmp = a.allocationPct - b.allocationPct;
          break;
      }
      return cmp * dir;
    });
  });

  function toggleSort(key: SortKey) {
    if (sortBy === key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortBy = key;
      sortDir = key === 'name' || key === 'ticker' ? 'asc' : 'desc';
    }
  }

  const fmt = formatSessionCurrency;

  const STALE_PRICE_DAYS = 7;

  function priceStaleDays(priceDate: string): number {
    return Math.floor((Date.now() - new Date(priceDate).getTime()) / 86_400_000);
  }
</script>

{#if loading}
  <TableSkeleton cols={10} rows={4} />
{:else if holdings.length === 0}
  <EmptyState card title="No holdings" description="No securities found in your GnuCash file.">
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <SortHead
          label="Holding"
          active={sortBy === 'name'}
          dir={sortDir}
          onclick={() => toggleSort('name')}
        />
        <SortHead
          label="Ticker"
          active={sortBy === 'ticker'}
          dir={sortDir}
          onclick={() => toggleSort('ticker')}
        />
        <SortHead
          label="Shares"
          active={sortBy === 'shares'}
          dir={sortDir}
          onclick={() => toggleSort('shares')}
          class="text-right"
        />
        <SortHead
          label="Avg Cost"
          active={sortBy === 'avgCost'}
          dir={sortDir}
          onclick={() => toggleSort('avgCost')}
          class="text-right"
        />
        <SortHead
          label="Price"
          active={sortBy === 'price'}
          dir={sortDir}
          onclick={() => toggleSort('price')}
          class="text-right"
        />
        <SortHead
          label="Day"
          active={sortBy === 'change'}
          dir={sortDir}
          onclick={() => toggleSort('change')}
          class="text-right"
        />
        <SortHead
          label="Market Value"
          active={sortBy === 'marketValue'}
          dir={sortDir}
          onclick={() => toggleSort('marketValue')}
          class="text-right"
        />
        <SortHead
          label="Cost Basis"
          active={sortBy === 'costBasis'}
          dir={sortDir}
          onclick={() => toggleSort('costBasis')}
          class="text-right"
        />
        <SortHead
          label="Gain / Loss"
          active={sortBy === 'gainLoss'}
          dir={sortDir}
          onclick={() => toggleSort('gainLoss')}
          class="text-right"
        />
        <SortHead
          label="Alloc"
          active={sortBy === 'alloc'}
          dir={sortDir}
          onclick={() => toggleSort('alloc')}
          class="text-right"
        />
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each sortedHoldings as h (h.commodityGuid)}
        {@const isClosed = h.shares === 0 && h.accounts.length > 0}
        {@const isWatchOnly = h.shares === 0 && h.accounts.length === 0}
        <Table.Row
          class="cursor-pointer {isClosed || isWatchOnly ? 'text-muted-foreground' : ''}"
          onclick={(e) => {
            // Anchor clicks must reach the document so SvelteKit's link
            // interceptor turns them into client-side navigations; a native
            // full-page load here would reboot the whole app.
            if ((e.target as HTMLElement).closest('a')) return;
            goto(resolve('/portfolio/[holding]', { holding: h.commodityGuid }));
          }}
        >
          <Table.Cell class="py-2.5">
            <!-- Real link for keyboard/AT users; the row click stays as a
                 convenience target for pointer input. -->
            <a
              href={resolve('/portfolio/[holding]', { holding: h.commodityGuid })}
              class="flex items-center gap-2 font-medium focus-visible:outline-2 focus-visible:outline-ring"
            >
              <span class="truncate">{h.fullName ?? h.mnemonic}</span>
              {#if isClosed}
                <Badge variant="outline" class="shrink-0 text-[10px] leading-none">Closed</Badge>
              {:else if isWatchOnly}
                <Badge variant="outline" class="shrink-0 text-[10px] leading-none">Not held</Badge>
              {/if}
            </a>
          </Table.Cell>
          <Table.Cell class="py-2.5 text-xs">{h.mnemonic}</Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">{h.shares.toFixed(2)}</Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {h.shares > 0 ? fmt(h.avgCostPerShare) : '\u2014'}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {#if h.price !== null}
              {@const stale =
                h.priceDate !== null && priceStaleDays(h.priceDate) > STALE_PRICE_DAYS}
              <span
                class={stale ? 'text-warning' : ''}
                title={h.priceDate === null ? undefined : formatDate(h.priceDate)}
              >
                {fmt(h.price)}
              </span>
            {:else}
              —
            {/if}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {#if dayChangePct(h) !== null}
              <span class={signColor(dayChangePct(h)!)}>{fmtPct(dayChangePct(h))}</span>
            {:else}
              —
            {/if}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right font-medium tabular-nums">
            {fmt(h.marketValue)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(h.costBasis)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums {signColor(h.gainLoss)}">
            {formatSignedCurrency(h.gainLoss, currency)}
            <span class="text-xs"> ({fmtPct(h.gainLossPct)})</span>
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {h.allocationPct.toFixed(1)}%
          </Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
