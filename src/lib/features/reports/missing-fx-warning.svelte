<script lang="ts">
  import { resolve } from '$app/paths';
  import { TriangleAlert } from '@lucide/svelte';
  import type { MissingPrice } from '$lib/tauri';
  import { plural } from '$lib/utils/format.svelte';

  let {
    missing = [],
    missingPrices = [],
    priceErrors = [],
    holdingGuidByMnemonic,
    reportCurrency = ''
  }: {
    missing?: string[];
    missingPrices?: MissingPrice[];
    /** Per-commodity Yahoo fetch failures (e.g. after a price refresh). */
    priceErrors?: { mnemonic: string; error: string }[];
    /** Maps mnemonic → holding page guid for "Fix symbol" links. */
    holdingGuidByMnemonic?: Map<string, string>;
    reportCurrency?: string;
  } = $props();
</script>

{#if missing.length > 0}
  <div
    role="alert"
    class="flex items-start gap-2.5 rounded-lg border border-warning/40 bg-warning/10 px-3.5 py-2.5 text-sm"
  >
    <TriangleAlert aria-hidden="true" class="mt-0.5 size-4 shrink-0 text-warning" />
    <p class="text-muted-foreground">
      <span class="font-medium text-foreground">Missing FX rates</span>
      for {missing.join(', ')}
      {#if reportCurrency}→ {reportCurrency}{/if}: converted at 1.0 parity, totals may be off.
      Refresh prices in Portfolio to load them from Yahoo Finance.
    </p>
  </div>
{/if}

{#if missingPrices.length > 0}
  <div
    role="alert"
    class="flex items-start gap-2.5 rounded-lg border border-warning/40 bg-warning/10 px-3.5 py-2.5 text-sm"
  >
    <TriangleAlert aria-hidden="true" class="mt-0.5 size-4 shrink-0 text-warning" />
    <div class="flex min-w-0 flex-col gap-1.5">
      <p class="text-muted-foreground">
        <span class="font-medium text-foreground">No Yahoo price</span>
        for {missingPrices.length === 1 ? 'this security' : 'these securities'} — shown at cost basis.
        Set an explicit symbol on the holding, then refresh prices.
      </p>
      <ul class="flex flex-col gap-1">
        {#each missingPrices as mp (mp.mnemonic)}
          <li class="flex flex-wrap items-center gap-x-2 text-xs">
            <span class="font-mono font-medium text-foreground">{mp.mnemonic}</span>
            {#if mp.commodityGuid}
              <a
                href={resolve('/portfolio/[holding]', { holding: mp.commodityGuid })}
                class="font-medium underline"
              >
                Set Yahoo symbol
              </a>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  </div>
{/if}

{#if priceErrors.length > 0}
  <div
    role="alert"
    class="flex items-start gap-2.5 rounded-lg border border-warning/40 bg-warning/10 px-3.5 py-2.5 text-sm"
  >
    <TriangleAlert aria-hidden="true" class="mt-0.5 size-4 shrink-0 text-warning" />
    <div class="flex min-w-0 flex-col gap-1.5">
      <p class="text-muted-foreground">
        <span class="font-medium text-foreground">
          {plural(priceErrors.length, 'price')} could not be fetched
        </span>
        — open the holding and set its Yahoo symbol explicitly.
      </p>
      <ul class="flex flex-col gap-1">
        {#each priceErrors as err (err.mnemonic)}
          <li class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs">
            <span class="font-mono font-medium text-foreground">{err.mnemonic}</span>
            <span class="min-w-0 truncate text-muted-foreground">{err.error}</span>
            {#if holdingGuidByMnemonic?.get(err.mnemonic)}
              <a
                href={resolve('/portfolio/[holding]', {
                  holding: holdingGuidByMnemonic.get(err.mnemonic) ?? ''
                })}
                class="font-medium underline"
              >
                Set Yahoo symbol
              </a>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  </div>
{/if}
