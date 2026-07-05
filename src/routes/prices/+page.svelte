<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { RefreshCw, Download } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import {
    refreshPrices,
    loadCommodityPageData,
    getPriceHistoryForCommodity
  } from '$lib/prices.remote';
  import { goto } from '$app/navigation';
  import { SvelteMap } from 'svelte/reactivity';
  import type {
    CommodityInfo,
    GnucashPriceEntry,
    PriceEntry,
    PriceHistoryEntry
  } from '$lib/server/prices';

  let commodities = $state<CommodityInfo[] | null>(null);
  let filePrices = $state<GnucashPriceEntry[]>([]);
  let livePrices = $state<PriceEntry[]>([]);
  let previousPrices = $state<{ mnemonic: string; price: number; date: string }[]>([]);
  let fetchedAt = $state<string | null>(null);
  let fetching = $state(false);
  let fetchError = $state<string | null>(null);
  let loadError = $state<string | null>(null);
  let errors = $state<{ mnemonic: string; error: string }[]>([]);

  let selectedGuid = $state<string | null>(null);
  let historyMap = $state<Record<string, PriceHistoryEntry[]>>({});
  let historyLoading = $state(false);

  let selectedCommodity = $derived(commodities?.find((c) => c.guid === selectedGuid) ?? null);

  let historyEntries = $derived(selectedGuid ? (historyMap[selectedGuid] ?? []) : []);

  let latestPrices = $derived.by(() => {
    const map = new SvelteMap<string, { price: number; currency: string; date: string }>();
    for (const fp of filePrices) {
      const existing = map.get(fp.mnemonic);
      if (!existing || fp.date > existing.date) {
        map.set(fp.mnemonic, { price: fp.price, currency: fp.currency, date: fp.date });
      }
    }
    for (const lp of livePrices) {
      const existing = map.get(lp.mnemonic);
      if (!existing || lp.priceDate > existing.date) {
        map.set(lp.mnemonic, { price: lp.price, currency: lp.currency, date: lp.priceDate });
      }
    }
    return map;
  });

  let previousPricesMap = $derived.by(() => {
    const map = new SvelteMap<string, { price: number; date: string }>();
    for (const pp of previousPrices) {
      map.set(pp.mnemonic, { price: pp.price, date: pp.date });
    }
    return map;
  });

  let monthlyHistory = $derived.by(() => {
    const grouped = new SvelteMap<string, PriceHistoryEntry[]>();
    for (const e of historyEntries) {
      const key = e.date.slice(0, 7);
      if (!grouped.has(key)) grouped.set(key, []);
      grouped.get(key)!.push(e);
    }
    return Array.from(grouped.entries())
      .sort(([a], [b]) => b.localeCompare(a))
      .map(([month, entries]) => {
        const latest = entries.reduce((a, b) => (a.date > b.date ? a : b));
        return {
          month,
          price: latest.price,
          currency: latest.currency,
          count: entries.length
        };
      });
  });

  $effect(() => {
    if (!dbSession.value) {
      commodities = null;
      filePrices = [];
      livePrices = [];
      previousPrices = [];
      selectedGuid = null;
      historyMap = {};
      return;
    }

    loadCommodityPageData({})
      .then((data) => {
        commodities = data.commodities;
        filePrices = data.filePrices;
        livePrices = data.livePrices;
        previousPrices = data.previousPrices;
        fetchedAt = data.lastRefreshed;
        if (data.commodities.length > 0 && !selectedGuid) {
          selectedGuid = data.commodities[0].guid;
        }
      })
      .catch((e: unknown) => {
        commodities = [];
        previousPrices = [];
        loadError = e instanceof Error ? e.message : 'Failed to load commodities';
      });
  });

  $effect(() => {
    const guid = selectedGuid;
    if (guid && !historyMap[guid]) {
      historyLoading = true;
      getPriceHistoryForCommodity({ commodityGuid: guid })
        .then((entries) => {
          historyMap = { ...historyMap, [guid]: entries };
        })
        .catch(() => {
          historyMap = { ...historyMap, [guid]: [] };
        })
        .finally(() => {
          historyLoading = false;
        });
    }
  });

  function selectCommodity(guid: string) {
    selectedGuid = selectedGuid === guid ? null : guid;
  }

  async function handleRefresh() {
    if (!dbSession.value) return;
    fetching = true;
    fetchError = null;
    errors = [];

    try {
      const overrides = appSettings.value.priceSymbolOverrides;
      const result = await refreshPrices({
        overrides: Object.keys(overrides).length > 0 ? overrides : undefined
      });
      livePrices = result.prices;
      fetchedAt = result.fetchedAt;
      errors = result.errors;
      // Reload history for selected commodity
      if (selectedGuid) {
        const entries = await getPriceHistoryForCommodity({
          commodityGuid: selectedGuid
        });
        historyMap = { ...historyMap, [selectedGuid]: entries };
      }
    } catch (e: unknown) {
      fetchError = e instanceof Error ? e.message : 'Failed to fetch prices';
    } finally {
      fetching = false;
    }
  }

  function formatPrice(price: number): string {
    return price.toLocaleString(appSettings.value.locale, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    });
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'short',
      day: 'numeric'
    });
  }

  function formatMonth(yearMonth: string): string {
    const [year, month] = yearMonth.split('-').map(Number);
    const d = new Date(year, month - 1);
    return d.toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'long'
    });
  }

  function downloadCsv() {
    const headers = ['Month', 'Price', 'Currency'];
    const rows = monthlyHistory.map((m) => [
      formatMonth(m.month),
      formatPrice(m.price),
      m.currency
    ]);
    const csv = [headers.join(','), ...rows.map((r) => r.join(','))].join('\n');

    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    const label = selectedCommodity?.mnemonic ?? 'prices';
    a.download = `${label}-price-history.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  $effect(() => {
    if (!dbSession.value) goto('/');
  });
</script>

<svelte:head>
  <title>Prices - Spendo</title>
</svelte:head>

<div class="flex flex-col gap-6">
  <!-- Header -->
  <div class="flex items-center justify-between gap-4">
    <div class="flex flex-col gap-1">
      <h2 class="text-3xl font-bold tracking-tight">Prices</h2>
      <p class="text-sm text-muted-foreground">
        Latest prices from your database
        {#if commodities}
          &middot; {commodities.length} commodity{commodities.length === 1 ? '' : 'ies'}
        {/if}
      </p>
    </div>
    <div class="flex items-center gap-2">
      <Button
        variant="outline"
        onclick={downloadCsv}
        disabled={!selectedCommodity || monthlyHistory.length === 0}
      >
        <Download aria-hidden="true" class="size-4" />
        CSV
      </Button>
      <Button onclick={handleRefresh} disabled={fetching || !commodities}>
        <RefreshCw aria-hidden="true" class="size-4 {fetching ? 'animate-spin' : ''}" />
        {fetching ? 'Refreshing...' : 'Refresh Prices'}
      </Button>
    </div>
  </div>

  {#if commodities === null}
    <!-- Loading skeleton -->
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {#each Array(3) as _, i (i)}
        <Card.Root>
          <Card.Content class="flex flex-col gap-3 pt-6">
            <div class="flex items-center justify-between">
              <Skeleton class="h-5 w-24" />
              <Skeleton class="h-4 w-12" />
            </div>
            <Skeleton class="h-8 w-28" />
            <Skeleton class="h-3 w-32" />
          </Card.Content>
        </Card.Root>
      {/each}
    </div>
  {:else if loadError}
    <Card.Root class="border-destructive">
      <Card.Content class="py-6 text-center text-sm text-destructive">
        {loadError}
      </Card.Content>
    </Card.Root>
  {:else if fetchError}
    <Card.Root class="border-destructive">
      <Card.Content class="py-6 text-center text-sm text-destructive">
        {fetchError}
      </Card.Content>
    </Card.Root>
  {:else if commodities.length === 0}
    <Card.Root>
      <Card.Content class="py-6 text-center text-sm text-muted-foreground">
        No non-currency commodities found in your GnuCash file.
      </Card.Content>
    </Card.Root>
  {:else}
    <!-- KPI cards -->
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {#each commodities as c (c.guid)}
        {@const lp = latestPrices.get(c.mnemonic)}
        <Card.Root
          class="cursor-pointer transition-all hover:shadow-md {selectedGuid === c.guid
            ? 'ring-2 ring-primary'
            : ''}"
          onclick={() => selectCommodity(c.guid)}
        >
          <Card.Content class="flex flex-col gap-3">
            <div class="flex items-center justify-between">
              <span class="font-semibold">{c.fullName || c.mnemonic}</span>
              <Badge variant="outline" class="text-[10px] leading-none text-muted-foreground">
                {c.namespace}
              </Badge>
            </div>

            {#if lp}
              <span class="text-3xl font-bold tracking-tight">
                {formatPrice(lp.price)}
              </span>
              {@const previous = previousPricesMap.get(c.mnemonic)}
              {#if previous && previous.price !== 0}
                {@const change = lp.price - previous.price}
                <p
                  class="text-xs {change >= 0
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : 'text-red-600 dark:text-red-400'}"
                >
                  {change >= 0 ? '+' : ''}{formatPrice(change)}&nbsp;({change >= 0 ? '+' : ''}
                  {((change / previous.price) * 100).toFixed(1)}%)
                </p>
              {/if}
              <p class="text-xs text-muted-foreground">
                {formatDate(lp.date)} &middot; {lp.currency}
              </p>
            {:else}
              <span class="text-3xl font-bold tracking-tight text-muted-foreground">—</span>
              <p class="text-xs text-muted-foreground">No price data</p>
            {/if}
          </Card.Content>
        </Card.Root>
      {/each}
    </div>

    <!-- History table -->
    {#if selectedCommodity}
      <Card.Root>
        <Card.Header>
          <Card.Title>{selectedCommodity.fullName || selectedCommodity.mnemonic}</Card.Title>
          <Card.Description>Monthly price history</Card.Description>
        </Card.Header>
        <Card.Content class="p-0">
          {#if historyLoading}
            <div class="divide-y">
              {#each Array(4) as _, i (i)}
                <div class="flex items-center gap-4 px-6 py-3">
                  <Skeleton class="h-4 w-28" />
                  <Skeleton class="h-4 w-20" />
                  <Skeleton class="h-4 w-12" />
                  <Skeleton class="h-4 w-16" />
                </div>
              {/each}
            </div>
          {:else if monthlyHistory.length === 0}
            <div class="py-6 text-center text-sm text-muted-foreground">
              No price history found.
            </div>
          {:else}
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b text-xs font-medium uppercase text-muted-foreground">
                  <th class="px-6 py-3 text-left">Month</th>
                  <th class="px-6 py-3 text-right">Price</th>
                  <th class="px-6 py-3 text-right">Currency</th>
                </tr>
              </thead>
              <tbody>
                {#each monthlyHistory as m (m.month)}
                  <tr class="border-b last:border-b-0">
                    <td class="px-6 py-3">{formatMonth(m.month)}</td>
                    <td class="px-6 py-3 text-right font-medium">
                      {formatPrice(m.price)}
                    </td>
                    <td class="px-6 py-3 text-right text-muted-foreground">
                      {m.currency}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </Card.Content>
      </Card.Root>
    {/if}

    <!-- Last refreshed -->
    {#if fetchedAt}
      <p class="text-xs text-muted-foreground">
        Last refreshed: {new Date(fetchedAt).toLocaleString()}
      </p>
    {/if}

    <!-- Errors -->
    {#if errors.length > 0}
      <Card.Root class="border-amber-300 dark:border-amber-700">
        <Card.Header>
          <Card.Title class="text-amber-700 dark:text-amber-400">Lookup Errors</Card.Title>
          <Card.Description>
            {errors.length} commodity{errors.length === 1 ? '' : 'ies'} could not be found
          </Card.Description>
        </Card.Header>
        <Card.Content>
          <ul class="list-disc pl-5 text-sm text-muted-foreground">
            {#each errors as e (e.mnemonic)}
              <li>
                <span class="font-medium">{e.mnemonic}</span>: {e.error}
              </li>
            {/each}
          </ul>
        </Card.Content>
      </Card.Root>
    {/if}
  {/if}
</div>
