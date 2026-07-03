<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { RefreshCw, Database, TrendingUp, TrendingDown, Minus } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { refreshPrices, loadCommodityPageData } from '$lib/prices.remote';
  import EmptyState from '$lib/components/empty-state.svelte';
  import type { CommodityInfo, GnucashPriceEntry, PriceEntry } from '$lib/server/prices';

  let commodities = $state<CommodityInfo[] | null>(null);
  let filePrices = $state<GnucashPriceEntry[]>([]);
  let livePrices = $state<PriceEntry[]>([]);
  let fetchedAt = $state<string | null>(null);
  let fetching = $state(false);
  let fetchError = $state<string | null>(null);
  let loadError = $state<string | null>(null);
  let errors = $state<{ mnemonic: string; error: string }[]>([]);

  $effect(() => {
    if (!dbSession.value) {
      commodities = null;
      filePrices = [];
      livePrices = [];
      return;
    }

    const token = dbSession.value.token;

    loadCommodityPageData({ token })
      .then((data) => {
        commodities = data.commodities;
        filePrices = data.filePrices;
      })
      .catch((e: unknown) => {
        loadError = e instanceof Error ? e.message : 'Failed to load commodities';
      });
  });

  async function handleRefresh() {
    if (!dbSession.value) return;
    fetching = true;
    fetchError = null;
    errors = [];

    try {
      const overrides = appSettings.value.priceSymbolOverrides;
      const result = await refreshPrices({
        token: dbSession.value.token,
        overrides: Object.keys(overrides).length > 0 ? overrides : undefined
      });
      livePrices = result.prices;
      fetchedAt = result.fetchedAt;
      errors = result.errors;
    } catch (e: unknown) {
      fetchError = e instanceof Error ? e.message : 'Failed to fetch prices';
    } finally {
      fetching = false;
    }
  }

  function filePriceFor(mnemonic: string): GnucashPriceEntry | undefined {
    return filePrices.find((p) => p.mnemonic === mnemonic);
  }

  function livePriceFor(mnemonic: string): PriceEntry | undefined {
    return livePrices.find((p) => p.mnemonic === mnemonic);
  }

  function priceChange(
    live: number,
    file: number | undefined
  ): { direction: 'up' | 'down' | 'same'; pct: string } | null {
    if (file == null) return null;
    if (file === 0) return null;
    const diff = ((live - file) / file) * 100;
    return {
      direction: diff > 0 ? 'up' : diff < 0 ? 'down' : 'same',
      pct: (diff >= 0 ? '+' : '') + diff.toFixed(2) + '%'
    };
  }

  function formatPrice(price: number): string {
    return price.toLocaleString(appSettings.value.locale, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    });
  }
</script>

<svelte:head>
  <title>Prices - Spendo</title>
</svelte:head>

{#if !dbSession.value}
  <EmptyState title="No Data" description="Upload a GnuCash file to see commodity prices.">
    {#snippet icon()}
      <Database aria-hidden="true" class="size-12 text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <div class="flex flex-col gap-6">
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <h2 class="text-3xl font-bold tracking-tight">Prices</h2>
        <p class="text-sm text-muted-foreground">
          Live market prices for your commodities
          {#if commodities}
            &middot; {commodities.length} commodity{commodities.length === 1 ? '' : 'ies'}
          {/if}
        </p>
      </div>

      <Button onclick={handleRefresh} disabled={fetching || !commodities}>
        <RefreshCw aria-hidden="true" class="size-4 {fetching ? 'animate-spin' : ''}" />
        {fetching ? 'Refreshing...' : 'Refresh Prices'}
      </Button>
    </div>

    {#if commodities === null}
      <!-- Loading skeleton -->
      <Card.Root>
        <Card.Content class="p-0">
          <div class="divide-y">
            {#each Array(5) as _, i (i)}
              <div class="flex items-center gap-4 px-6 py-3">
                <div class="flex-1">
                  <Skeleton class="h-4 w-32" />
                  <Skeleton class="mt-1 h-3 w-20" />
                </div>
                <Skeleton class="h-4 w-24" />
                <Skeleton class="h-4 w-24" />
                <Skeleton class="h-4 w-16" />
              </div>
            {/each}
          </div>
        </Card.Content>
      </Card.Root>
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
      <!-- Price table -->
      <Card.Root>
        <Card.Content class="p-0">
          <!-- Header row -->
          <div
            class="hidden border-b px-6 py-2 text-xs font-medium uppercase text-muted-foreground md:flex"
          >
            <div class="flex-1">Commodity</div>
            <div class="w-28 text-right">GnuCash Price</div>
            <div class="w-28 text-right">Current Price</div>
            <div class="w-24 text-right">Change</div>
          </div>
          <div class="divide-y">
            {#each commodities as c (c.guid)}
              {@const fp = filePriceFor(c.mnemonic)}
              {@const lp = livePriceFor(c.mnemonic)}
              {@const ch = lp ? priceChange(lp.price, fp?.price) : null}
              <div
                class="flex flex-col gap-1 px-6 py-3 text-sm md:flex-row md:items-center md:gap-0"
              >
                <div class="flex-1">
                  <span class="font-medium">{c.fullName || c.mnemonic}</span>
                  <Badge variant="outline" class="text-[10px] leading-none text-muted-foreground">
                    {c.namespace}
                  </Badge>
                </div>
                <div class="text-muted-foreground md:w-28 md:text-right">
                  <span class="md:hidden text-[10px] uppercase">GnuCash: </span>
                  {fp ? formatPrice(fp.price) : '—'}
                  {#if fp}
                    <span class="text-[10px] text-muted-foreground"> {fp.currency}</span>
                  {/if}
                </div>
                <div class="md:w-28 md:text-right">
                  <span class="md:hidden text-[10px] uppercase">Current: </span>
                  {#if lp}
                    {formatPrice(lp.price)}
                  {:else if fetching}
                    <span class="text-muted-foreground">—</span>
                  {:else}
                    <span class="text-muted-foreground">Not fetched</span>
                  {/if}
                </div>
                <div class="md:w-24 md:text-right">
                  {#if ch}
                    <span
                      class="inline-flex items-center gap-0.5 {ch.direction === 'up'
                        ? 'text-emerald-600 dark:text-emerald-400'
                        : ch.direction === 'down'
                          ? 'text-red-600 dark:text-red-400'
                          : 'text-muted-foreground'}"
                    >
                      {#if ch.direction === 'up'}
                        <TrendingUp class="size-3" />
                      {:else if ch.direction === 'down'}
                        <TrendingDown class="size-3" />
                      {:else}
                        <Minus class="size-3" />
                      {/if}
                      {ch.pct}
                    </span>
                  {:else if lp}
                    <span class="text-muted-foreground">New</span>
                  {:else}
                    —
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </Card.Content>
      </Card.Root>

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
{/if}
