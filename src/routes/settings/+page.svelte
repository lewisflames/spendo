<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Badge } from '$lib/components/ui/badge';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import {
    appSettings,
    saveSettings,
    resetSettings,
    type Locale,
    type DashboardRange,
    type FirstDayOfWeek
  } from '$lib/stores/settings.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { getCommodities } from '$lib/prices.remote';
  import { SunMoon, Languages, ListOrdered, LayoutDashboard, Tag, X } from '@lucide/svelte';
  import type { CommodityInfo } from '$lib/server/prices';

  const locales: { value: Locale; label: string }[] = [
    { value: 'en-US', label: 'English (US)' },
    { value: 'de-DE', label: 'Deutsch (DE)' },
    { value: 'fr-FR', label: 'Français (FR)' },
    { value: 'ja-JP', label: '日本語 (JP)' },
    { value: 'pt-BR', label: 'Português (BR)' },
    { value: 'es-ES', label: 'Español (ES)' }
  ];

  const pageSizeOptions = ['25', '50', '100'] as const;

  const dashRangeOptions: { value: DashboardRange; label: string }[] = [
    { value: '1M', label: '1 month' },
    { value: '3M', label: '3 months' },
    { value: '6M', label: '6 months' },
    { value: '1Y', label: '1 year' },
    { value: 'All', label: 'All time' }
  ];

  let locale = $state(appSettings.value.locale);
  let theme = $state(appSettings.value.theme);
  let pageSize = $state(String(appSettings.value.pageSize));
  let dashboardRange = $state(appSettings.value.dashboardRange);
  let firstDayOfWeek = $state(String(appSettings.value.firstDayOfWeek));

  let commodities = $state<CommodityInfo[]>([]);
  let overrideSymbols = $state<Record<string, string>>({
    ...appSettings.value.priceSymbolOverrides
  });
  let commoditiesLoading = $state(false);
  let commoditiesError = $state<string | null>(null);

  $effect(() => {
    if (!dbSession.value) {
      commodities = [];
      overrideSymbols = { ...appSettings.value.priceSymbolOverrides };
      return;
    }
    commoditiesLoading = true;
    commoditiesError = null;
    getCommodities({ token: dbSession.value.token })
      .then((comms) => {
        commodities = comms;
      })
      .catch((e: unknown) => {
        commoditiesError = e instanceof Error ? e.message : 'Failed to load commodities';
      })
      .finally(() => {
        commoditiesLoading = false;
      });
  });

  function defaultSymbol(mnemonic: string, namespace: string): string {
    const suffixMap: Record<string, string[]> = {
      EUREX: ['.DE'],
      XETRA: ['.DE'],
      EURONEXT: ['.AS', '.PA'],
      FUND: ['.AS', '.L', '.DE', '.PA'],
      MUTF: ['.AS', '.L', '.PA'],
      CRYPTO: ['-EUR'],
      NYSE: [''],
      NASDAQ: [''],
      AMEX: ['']
    };
    const suffixes = suffixMap[namespace] ?? ['.DE', '.AS', '.L', '.PA', ''];
    return suffixes[0] ? `${mnemonic}${suffixes[0]}` : mnemonic;
  }

  let dirty = $derived(
    locale !== appSettings.value.locale ||
      theme !== appSettings.value.theme ||
      Number(pageSize) !== appSettings.value.pageSize ||
      dashboardRange !== appSettings.value.dashboardRange ||
      Number(firstDayOfWeek) !== appSettings.value.firstDayOfWeek ||
      JSON.stringify(overrideSymbols) !== JSON.stringify(appSettings.value.priceSymbolOverrides)
  );

  function handleSave() {
    saveSettings({
      locale,
      theme,
      pageSize: Number(pageSize),
      dashboardRange,
      firstDayOfWeek: Number(firstDayOfWeek) as FirstDayOfWeek,
      priceSymbolOverrides: overrideSymbols
    });
  }

  function handleReset() {
    resetSettings();
    locale = appSettings.value.locale;
    theme = appSettings.value.theme;
    pageSize = String(appSettings.value.pageSize);
    dashboardRange = appSettings.value.dashboardRange;
    firstDayOfWeek = String(appSettings.value.firstDayOfWeek);
    overrideSymbols = { ...appSettings.value.priceSymbolOverrides };
  }
</script>

<svelte:head>
  <title>Settings — Spendo</title>
  <meta name="description" content="Configure Spendo preferences" />
</svelte:head>

<div class="flex flex-col gap-6">
  <div class="flex flex-col gap-1">
    <h2 class="text-3xl font-bold tracking-tight">Settings</h2>
    <p class="text-sm text-muted-foreground">Configure your preferences</p>
  </div>

  <Card.Root>
    <Card.Header>
      <div class="flex items-center gap-2">
        <SunMoon aria-hidden="true" class="size-4 text-muted-foreground" />
        <Card.Title>Appearance</Card.Title>
      </div>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Theme</span>
          <span class="text-xs text-muted-foreground">Switch between light and dark mode</span>
        </div>
        <Select.Root type="single" bind:value={theme}>
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select theme" />
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="light">Light</Select.Item>
            <Select.Item value="dark">Dark</Select.Item>
          </Select.Content>
        </Select.Root>
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <div class="flex items-center gap-2">
        <Languages aria-hidden="true" class="size-4 text-muted-foreground" />
        <Card.Title>Regional</Card.Title>
      </div>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Locale</span>
          <span class="text-xs text-muted-foreground">
            Date, number and currency display format
          </span>
        </div>
        <Select.Root type="single" bind:value={locale}>
          <Select.Trigger class="w-44">
            <SelectPrimitive.Value placeholder="Select locale" />
          </Select.Trigger>
          <Select.Content>
            {#each locales as l (l.value)}
              <Select.Item value={l.value}>{l.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">First day of week</span>
          <span class="text-xs text-muted-foreground"> Start of the week in calendars </span>
        </div>
        <Select.Root type="single" bind:value={firstDayOfWeek}>
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select day" />
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="0">Sunday</Select.Item>
            <Select.Item value="1">Monday</Select.Item>
          </Select.Content>
        </Select.Root>
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <div class="flex items-center gap-2">
        <ListOrdered aria-hidden="true" class="size-4 text-muted-foreground" />
        <Card.Title>Transactions</Card.Title>
      </div>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Page size</span>
          <span class="text-xs text-muted-foreground">Transactions per page</span>
        </div>
        <Select.Root type="single" bind:value={pageSize}>
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select size" />
          </Select.Trigger>
          <Select.Content>
            {#each pageSizeOptions as opt (opt)}
              <Select.Item value={opt}>{opt} per page</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <div class="flex items-center gap-2">
        <LayoutDashboard aria-hidden="true" class="size-4 text-muted-foreground" />
        <Card.Title>Dashboard</Card.Title>
      </div>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Default time range</span>
          <span class="text-xs text-muted-foreground">
            Initial time range shown on the dashboard
          </span>
        </div>
        <Select.Root type="single" bind:value={dashboardRange}>
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select range" />
          </Select.Trigger>
          <Select.Content>
            {#each dashRangeOptions as opt (opt.value)}
              <Select.Item value={opt.value}>{opt.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <div class="flex items-center gap-2">
        <Tag aria-hidden="true" class="size-4 text-muted-foreground" />
        <Card.Title>Price Symbol Overrides</Card.Title>
      </div>
      <Card.Description>
        Override Yahoo Finance symbols for commodities that aren't found automatically. This section
        requires an active GnuCash session.
      </Card.Description>
    </Card.Header>
    <Card.Content>
      {#if !dbSession.value}
        <p class="text-sm text-muted-foreground">
          Upload a GnuCash file to configure symbol overrides.
        </p>
      {:else if commoditiesLoading}
        <div class="flex flex-col gap-3">
          {#each Array(3) as _, i (i)}
            <div class="flex items-center gap-3">
              <div class="flex-1">
                <Skeleton class="h-4 w-32" />
                <Skeleton class="mt-1 h-3 w-20" />
              </div>
              <Skeleton class="h-4 w-28" />
              <Skeleton class="h-4 w-32" />
            </div>
          {/each}
        </div>
      {:else if commoditiesError}
        <p class="text-sm text-destructive">{commoditiesError}</p>
      {:else if commodities.length === 0}
        <p class="text-sm text-muted-foreground">
          No non-currency commodities found in your GnuCash file.
        </p>
      {:else}
        <div
          class="hidden border-b pb-2 text-xs font-medium uppercase text-muted-foreground md:flex md:items-center"
        >
          <div class="flex-1">Commodity</div>
          <div class="w-28">Default symbol</div>
          <div class="w-32">Override</div>
        </div>
        <div class="mt-2 flex flex-col gap-2">
          {#each commodities as c (c.guid)}
            <div class="flex flex-col gap-1 md:flex-row md:items-center md:gap-3">
              <div class="flex-1">
                <span class="text-sm font-medium">{c.fullName || c.mnemonic}</span>
                <Badge
                  variant="outline"
                  class="ml-1.5 text-[10px] leading-none text-muted-foreground"
                >
                  {c.namespace}
                </Badge>
              </div>
              <div class="text-sm text-muted-foreground md:w-28">
                <code class="text-xs">{defaultSymbol(c.mnemonic, c.namespace)}</code>
              </div>
              <div class="flex items-center gap-1 md:w-32">
                <Input
                  type="text"
                  placeholder={defaultSymbol(c.mnemonic, c.namespace)}
                  bind:value={overrideSymbols[c.guid]}
                  class="h-7 text-xs"
                />
                {#if overrideSymbols[c.guid]?.length > 0}
                  <button
                    type="button"
                    onclick={() => {
                      const copy = { ...overrideSymbols };
                      delete copy[c.guid];
                      overrideSymbols = copy;
                    }}
                    class="inline-flex items-center justify-center rounded p-1 text-muted-foreground hover:text-foreground"
                    aria-label="Clear override"
                  >
                    <X class="size-3" />
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>

  <div class="flex items-center gap-3">
    <Button onclick={handleSave} disabled={!dirty}>Save changes</Button>
    <Button variant="outline" onclick={handleReset} disabled={!dirty}>Reset to defaults</Button>
  </div>
</div>
