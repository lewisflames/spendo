<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import { Select as SelectPrimitive } from 'bits-ui';
  import {
    appSettings,
    saveSettings,
    resetSettings,
    PAGE_SIZE_OPTIONS,
    type AccentId,
    type Density,
    type Locale,
    type Theme
  } from '$lib/stores/settings.svelte';
  import {
    clearAppDb,
    createBookAt,
    fetchDataInfo,
    openBook,
    saveBookAs,
    setBaseCurrency
  } from '$lib/tauri';
  import { BOOK_CURRENCIES, defaultCurrencyForLocale } from '$lib/utils/currency';
  import { ensureSpendoExtension } from '$lib/utils/book';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { resolve } from '$app/paths';
  import {
    SunMoon,
    ListOrdered,
    HardDrive,
    FolderOpen,
    Palette,
    Plus,
    Save,
    RotateCcw,
    Trash2,
    Copy,
    Check,
    ExternalLink,
    CalendarDays
  } from '@lucide/svelte';
  import { cn } from '$lib/utils.js';
  import * as Dialog from '$lib/components/ui/dialog';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DataExport from '$lib/features/settings/data-export.svelte';

  import {
    dbSession,
    activeBookPath,
    clearSession,
    setActiveBookPath,
    setDbSession
  } from '$lib/stores/db.svelte';
  import {
    bookSettings,
    loadBookSettings,
    resetBookSettings,
    saveBookSetting
  } from '$lib/stores/book-settings.svelte';
  import type { BookSettings, PnlPeriodId } from '$lib/tauri';
  import { monthName } from '$lib/utils/date-utils.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  const locales: { value: Locale; label: string }[] = [
    { value: 'en-US', label: 'English (US)' },
    { value: 'en-GB', label: 'English (GB)' },
    { value: 'en-IE', label: 'English (IE)' },
    { value: 'de-DE', label: 'Deutsch (DE)' },
    { value: 'fr-FR', label: 'Français (FR)' },
    { value: 'ja-JP', label: '日本語 (JP)' },
    { value: 'pt-BR', label: 'Português (BR)' },
    { value: 'es-ES', label: 'Español (ES)' }
  ];

  const themes: { value: Theme; label: string }[] = [
    { value: 'system', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' }
  ];

  const densities: { value: Density; label: string }[] = [
    { value: 'comfortable', label: 'Comfortable' },
    { value: 'compact', label: 'Compact' }
  ];

  const pnlPeriods: { value: PnlPeriodId; label: string }[] = [
    { value: 'this_month', label: 'This month' },
    { value: 'this_quarter', label: 'This quarter' },
    { value: 'this_year', label: 'This year (fiscal)' },
    { value: 'last_12_months', label: 'Last 12 months' }
  ];

  const lookaheadOptions: { value: string; label: string }[] = [
    { value: '7', label: 'Next 7 days' },
    { value: '14', label: 'Next 14 days' },
    { value: '30', label: 'Next 30 days' },
    { value: '60', label: 'Next 60 days' },
    { value: '90', label: 'Next 90 days' }
  ];

  const DEFAULT_CUSTOM_PRIMARY = '#3b82f6';

  const accents: { value: AccentId; label: string; swatch: string }[] = [
    { value: 'zinc', label: 'Zinc', swatch: '#71717a' },
    { value: 'blue', label: 'Blue', swatch: '#2563eb' },
    { value: 'emerald', label: 'Emerald', swatch: '#059669' },
    { value: 'amber', label: 'Amber', swatch: '#d97706' },
    { value: 'rose', label: 'Rose', swatch: '#e11d48' },
    { value: 'violet', label: 'Violet', swatch: '#7c3aed' },
    { value: 'custom', label: 'Custom', swatch: '' }
  ];

  let customDraft = $state(appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY);
  let customValid = $derived(/^#[0-9a-fA-F]{6}$/.test(customDraft.trim()));

  function handleAccentSelect(value: AccentId) {
    if (value === 'custom') {
      const seeded = appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY;
      customDraft = seeded;
      saveSettings({ accent: 'custom', customPrimary: seeded });
    } else {
      saveSettings({ accent: value });
    }
  }

  function handleCustomPick(value: string) {
    customDraft = value;
    saveSettings({ accent: 'custom', customPrimary: value });
  }

  function handleCustomHexInput(value: string) {
    customDraft = value;
    const hex = value.trim();
    if (/^#[0-9a-fA-F]{6}$/.test(hex)) saveSettings({ accent: 'custom', customPrimary: hex });
  }

  let clearDialogOpen = $state(false);
  let resetDialogOpen = $state(false);
  let currencyDialogOpen = $state(false);
  let pendingCurrency = $state<string | null>(null);
  let clearing = $state(false);
  let savingCurrency = $state(false);
  let switching = $state(false);
  let copied = $state(false);
  let bookSettingsError = $state<string | null>(null);
  let savingBookKey = $state(false);

  $effect(() => {
    loadBookSettings().catch((e: unknown) => {
      bookSettingsError = errMsg(e, 'Failed to load book settings');
    });
  });

  const fyMonths = $derived(
    Array.from({ length: 12 }, (_, i) => ({
      value: String(i + 1),
      label: monthName(i + 1, appSettings.value.locale)
    }))
  );

  async function handleBookSetting<
    K extends
      'fiscalYearStartMonth' | 'defaultPnlPeriod' | 'priceAutoRefresh' | 'remindersLookaheadDays'
  >(field: K, value: BookSettings[K]) {
    if (!dbSession.value || savingBookKey) return;
    savingBookKey = true;
    try {
      await saveBookSetting(field, value);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to save book setting'));
    } finally {
      savingBookKey = false;
    }
  }

  async function handleOpenBook() {
    if (switching) return;
    let selected: string | string[] | null;
    try {
      selected = await open({
        multiple: false,
        filters: [{ name: 'Spendo book', extensions: ['spendo', 'db', 'sqlite'] }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open file dialog'));
      return;
    }
    if (!selected || Array.isArray(selected)) return;
    switching = true;
    try {
      const result = await openBook(selected);
      setDbSession(result.summary);
      setActiveBookPath(selected);
      bookSettingsError = null;
      await loadBookSettings(true).catch((e: unknown) => {
        bookSettingsError = errMsg(e, 'Failed to load book settings');
      });
      toast.success(`Opened ${await basename(selected)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open book'));
    } finally {
      switching = false;
    }
  }

  async function handleNewBook() {
    if (switching) return;
    let target: string | null;
    try {
      target = await save({
        defaultPath: 'book.spendo',
        filters: [{ name: 'Spendo book', extensions: ['spendo'] }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return;
    }
    if (!target) return;
    const file = ensureSpendoExtension(target);
    const currency = defaultCurrencyForLocale(appSettings.value.locale);
    switching = true;
    try {
      const result = await createBookAt(file, currency);
      setDbSession(result.summary);
      setActiveBookPath(file);
      bookSettingsError = null;
      await loadBookSettings(true).catch((e: unknown) => {
        bookSettingsError = errMsg(e, 'Failed to load book settings');
      });
      toast.success(`Book created (${currency})`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to create book'));
    } finally {
      switching = false;
    }
  }

  async function handleSaveAs() {
    if (switching || !dbSession.value) return;
    let target: string | null;
    try {
      target = await save({
        defaultPath: 'book.spendo',
        filters: [{ name: 'Spendo book', extensions: ['spendo'] }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return;
    }
    if (!target) return;
    const file = ensureSpendoExtension(target);
    switching = true;
    try {
      const result = await saveBookAs(file);
      setDbSession(result.summary);
      setActiveBookPath(file);
      bookSettingsError = null;
      await loadBookSettings(true).catch((e: unknown) => {
        bookSettingsError = errMsg(e, 'Failed to load book settings');
      });
      toast.success(`Book saved as ${await basename(file)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to save book'));
    } finally {
      switching = false;
    }
  }

  const dataRes = useAsyncData({
    enabled: () => true,
    fetch: fetchDataInfo,
    label: 'Failed to load data location'
  });

  const activePath = $derived(activeBookPath.value ?? dataRes.data?.dbPath ?? null);

  function bookHasData(): boolean {
    const summary = dbSession.value?.summary;
    return !!summary && (summary.totalAccounts > 0 || summary.totalTransactions > 0);
  }

  function requestCurrencyChange(v: string | undefined) {
    if (!v || !dbSession.value || v === dbSession.value.summary.currency) return;
    // Revaluing a populated book touches every report, so confirm first.
    // Empty books can switch silently.
    if (bookHasData()) {
      pendingCurrency = v;
      currencyDialogOpen = true;
    } else {
      void handleCurrencyChange(v);
    }
  }

  async function confirmCurrencyChange() {
    if (!pendingCurrency) return;
    const next = pendingCurrency;
    pendingCurrency = null;
    currencyDialogOpen = false;
    await handleCurrencyChange(next);
  }

  async function handleCurrencyChange(v: string) {
    savingCurrency = true;
    try {
      const result = await setBaseCurrency(v);
      setDbSession(result.summary);
      toast.success(`Base currency set to ${v}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to set base currency'));
    } finally {
      savingCurrency = false;
    }
  }

  async function handleClearDb() {
    clearing = true;
    try {
      await clearAppDb();
      clearSession();
      resetBookSettings();
      clearDialogOpen = false;
      toast.success('Database cleared');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to clear database'));
    } finally {
      clearing = false;
    }
  }

  async function handleRevealDb() {
    if (!activePath) return;
    try {
      await revealItemInDir(activePath);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open file manager'));
    }
  }

  async function handleCopyPath() {
    if (!activePath) return;
    try {
      await navigator.clipboard.writeText(activePath);
      copied = true;
      toast.success('Book path copied');
      setTimeout(() => (copied = false), 1500);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to copy path'));
    }
  }

  function handleReset() {
    resetSettings();
    customDraft = DEFAULT_CUSTOM_PRIMARY;
    resetDialogOpen = false;
    toast.success('Preferences reset to defaults');
  }
</script>

<svelte:head>
  <title>Settings — Spendo</title>
  <meta name="description" content="Configure Spendo preferences" />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader title="Settings" description="Configure your preferences" />

  <Card.Root>
    <IconCardHeader title="Appearance">
      {#snippet icon()}
        <SunMoon aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Theme</span>
          <span class="text-xs text-muted-foreground">Follow the system or pick light / dark</span>
        </div>
        <Select.Root
          type="single"
          value={appSettings.value.theme}
          onValueChange={(v) => {
            if (v === 'light' || v === 'dark' || v === 'system') saveSettings({ theme: v });
          }}
          items={themes}
        >
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select theme" />
          </Select.Trigger>
          <Select.Content>
            {#each themes as t (t.value)}
              <Select.Item value={t.value}>{t.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Accent</span>
          <span class="text-xs text-muted-foreground">Preset color or your own custom color</span>
        </div>
        <div class="flex items-center gap-1.5" role="radiogroup" aria-label="Accent color">
          {#each accents as a (a.value)}
            <button
              type="button"
              role="radio"
              aria-checked={appSettings.value.accent === a.value}
              title={a.label}
              aria-label={a.label}
              onclick={() => handleAccentSelect(a.value)}
              class={cn(
                'flex size-7 items-center justify-center rounded-full border transition-all outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
                appSettings.value.accent === a.value
                  ? 'border-primary ring-2 ring-ring/50'
                  : 'border-border hover:scale-110 hover:border-muted-foreground'
              )}
            >
              {#if a.value === 'custom'}
                <span
                  aria-hidden="true"
                  class="flex size-5 items-center justify-center rounded-full"
                  style="background: conic-gradient(from 0deg, #f43f5e, #f59e0b, #10b981, #3b82f6, #8b5cf6, #f43f5e)"
                >
                  <Palette aria-hidden="true" class="size-3 text-white drop-shadow" />
                </span>
              {:else}
                <span
                  aria-hidden="true"
                  class="size-5 rounded-full"
                  style="background-color: {a.swatch}"
                ></span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
      {#if appSettings.value.accent === 'custom'}
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-1">
            <span class="text-sm font-medium">Custom color</span>
            <span class="text-xs text-muted-foreground">Applied to buttons, rings and charts</span>
          </div>
          <div class="flex items-center gap-2">
            <input
              type="color"
              aria-label="Pick a custom accent color"
              value={appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY}
              oninput={(e) => handleCustomPick(e.currentTarget.value)}
              class="h-8 w-10 cursor-pointer rounded-md border border-input bg-transparent p-1"
            />
            <Input
              id="custom-primary-hex"
              value={customDraft}
              oninput={(e) => handleCustomHexInput(e.currentTarget.value)}
              aria-invalid={!customValid}
              aria-label="Custom accent color hex value"
              placeholder="#3b82f6"
              maxlength={7}
              spellcheck={false}
              class="w-24 font-mono"
            />
          </div>
        </div>
        {#if !customValid}
          <p aria-live="polite" class="text-xs text-destructive">Enter a hex color like #3b82f6.</p>
        {/if}
      {/if}
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Density</span>
          <span class="text-xs text-muted-foreground">Tighter spacing for pages and tables</span>
        </div>
        <Select.Root
          type="single"
          value={appSettings.value.density}
          onValueChange={(v) => {
            if (v === 'comfortable' || v === 'compact') saveSettings({ density: v });
          }}
          items={densities}
        >
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select density" />
          </Select.Trigger>
          <Select.Content>
            {#each densities as d (d.value)}
              <Select.Item value={d.value}>{d.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Sidebar</span>
          <span class="text-xs text-muted-foreground">
            Start with the navigation collapsed (Ctrl+B toggles)
          </span>
        </div>
        <Switch
          checked={appSettings.value.sidebarCollapsed}
          onCheckedChange={(v) => saveSettings({ sidebarCollapsed: v })}
          aria-label="Start with sidebar collapsed"
        />
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <IconCardHeader title="Display & lists">
      {#snippet icon()}
        <ListOrdered aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Locale</span>
          <span class="text-xs text-muted-foreground">
            Date, number and currency display format
          </span>
        </div>
        <Select.Root
          type="single"
          value={appSettings.value.locale}
          onValueChange={(v) => saveSettings({ locale: v as Locale })}
          items={locales}
        >
          <Select.Trigger class="w-48">
            <SelectPrimitive.Value placeholder="Select locale" />
          </Select.Trigger>
          <Select.Content>
            {#each locales as l (l.value)}
              <Select.Item value={l.value} label={l.label}>{l.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Rows per page</span>
          <span class="text-xs text-muted-foreground">
            Applies to every list: transactions, ledger, history, holdings
          </span>
        </div>
        <Select.Root
          type="single"
          value={String(appSettings.value.pageSize)}
          onValueChange={(v) => saveSettings({ pageSize: Number(v) })}
          items={PAGE_SIZE_OPTIONS.map((opt) => ({ value: String(opt), label: `${opt} per page` }))}
        >
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select size" />
          </Select.Trigger>
          <Select.Content>
            {#each PAGE_SIZE_OPTIONS as opt (opt)}
              <Select.Item value={String(opt)} label={`${opt} per page`}>{opt} per page</Select.Item
              >
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
    </Card.Content>
  </Card.Root>

  <!-- Book -->
  <Card.Root>
    <IconCardHeader title="Book" description="Your book is a portable .spendo file.">
      {#snippet icon()}
        <HardDrive aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Base currency</span>
          <span class="text-xs text-muted-foreground">
            Report currency for the book — revalues net worth, reports and portfolio
          </span>
        </div>
        {#if dbSession.value}
          <Select.Root
            type="single"
            value={dbSession.value.summary.currency}
            onValueChange={requestCurrencyChange}
            items={BOOK_CURRENCIES.map((c) => ({ value: c, label: c }))}
            disabled={savingCurrency}
          >
            <Select.Trigger class="w-36">
              <SelectPrimitive.Value placeholder="Select currency" />
            </Select.Trigger>
            <Select.Content>
              {#each BOOK_CURRENCIES as c (c)}
                <Select.Item value={c} label={c}>{c}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        {:else}
          <span class="text-xs text-muted-foreground">
            No book yet — start from <a href={resolve('/')} class="underline">Welcome</a>.
          </span>
        {/if}
      </div>
      <div class="flex flex-col items-start gap-3">
        {#if dataRes.error}
          <ErrorCard message={dataRes.error} onRetry={() => dataRes.reload()} />
        {:else}
          <div class="flex w-full items-center gap-2">
            <code
              class="max-w-full flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-xs text-muted-foreground"
              title={activePath ?? undefined}
            >
              {activePath ?? 'No book open'}
            </code>
            <Button
              variant="outline"
              size="icon"
              disabled={!activePath}
              onclick={handleCopyPath}
              aria-label="Copy book path"
              title="Copy book path"
            >
              {#if copied}
                <Check aria-hidden="true" class="size-4" />
              {:else}
                <Copy aria-hidden="true" class="size-4" />
              {/if}
            </Button>
          </div>
          <div class="flex flex-wrap gap-2">
            <Button variant="outline" disabled={switching} onclick={handleOpenBook}>
              <FolderOpen aria-hidden="true" class="size-4" />
              {switching ? 'Working...' : 'Open...'}
            </Button>
            <Button
              variant="outline"
              disabled={switching || !dbSession.value}
              onclick={handleSaveAs}
            >
              <Save aria-hidden="true" class="size-4" />
              Save as...
            </Button>
            <Button variant="outline" disabled={switching} onclick={handleNewBook}>
              <Plus aria-hidden="true" class="size-4" />
              New...
            </Button>
            <Button variant="outline" disabled={!activePath} onclick={handleRevealDb}>
              <ExternalLink aria-hidden="true" class="size-4" />
              Reveal in file manager
            </Button>
          </div>
        {/if}
      </div>
    </Card.Content>
  </Card.Root>

  <Dialog.Root bind:open={currencyDialogOpen}>
    <Dialog.Content>
      <Dialog.Header>
        <Dialog.Title>Change base currency?</Dialog.Title>
        <Dialog.Description>
          This revalues net worth, P&amp;L reports and portfolio totals to {pendingCurrency ?? ''}.
          Past transactions keep their original amounts; only the report currency changes.
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button
          variant="outline"
          onclick={() => {
            currencyDialogOpen = false;
            pendingCurrency = null;
          }}
          disabled={savingCurrency}
        >
          Cancel
        </Button>
        <Button onclick={confirmCurrencyChange} disabled={savingCurrency || !pendingCurrency}>
          {savingCurrency ? 'Saving...' : `Change to ${pendingCurrency ?? ''}`}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Reporting (book-scoped: travels with the .spendo file) -->
  <Card.Root>
    <IconCardHeader
      title="Reporting"
      description="Book-scoped defaults stored inside the .spendo file."
    >
      {#snippet icon()}
        <CalendarDays aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      {#if bookSettingsError}
        <ErrorCard
          message={bookSettingsError}
          onRetry={() => void loadBookSettings(true).catch(() => {})}
        />
      {/if}
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Fiscal year starts</span>
          <span class="text-xs text-muted-foreground">
            Shapes the This-year P&amp;L default; full report grouping follows next
          </span>
        </div>
        {#if dbSession.value}
          <Select.Root
            type="single"
            value={String(bookSettings.value.fiscalYearStartMonth)}
            onValueChange={(v) => {
              const month = Number(v);
              if (Number.isInteger(month) && month >= 1 && month <= 12)
                void handleBookSetting('fiscalYearStartMonth', month);
            }}
            items={fyMonths}
            disabled={savingBookKey}
          >
            <Select.Trigger class="w-36">
              <SelectPrimitive.Value placeholder="Select month" />
            </Select.Trigger>
            <Select.Content>
              {#each fyMonths as m (m.value)}
                <Select.Item value={m.value} label={m.label}>{m.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        {:else}
          <span class="text-xs text-muted-foreground">No book open.</span>
        {/if}
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Default P&amp;L period</span>
          <span class="text-xs text-muted-foreground">
            Initial range for Profit &amp; Loss when no dates are in the URL
          </span>
        </div>
        {#if dbSession.value}
          <Select.Root
            type="single"
            value={bookSettings.value.defaultPnlPeriod}
            onValueChange={(v) => {
              if (
                v === 'this_month' ||
                v === 'this_quarter' ||
                v === 'this_year' ||
                v === 'last_12_months'
              )
                void handleBookSetting('defaultPnlPeriod', v);
            }}
            items={pnlPeriods}
            disabled={savingBookKey}
          >
            <Select.Trigger class="w-44">
              <SelectPrimitive.Value placeholder="Select period" />
            </Select.Trigger>
            <Select.Content>
              {#each pnlPeriods as p (p.value)}
                <Select.Item value={p.value} label={p.label}>{p.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        {:else}
          <span class="text-xs text-muted-foreground">No book open.</span>
        {/if}
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Auto-refresh prices</span>
          <span class="text-xs text-muted-foreground">
            Fetch fresh Yahoo prices when opening the Portfolio
          </span>
        </div>
        <Switch
          checked={bookSettings.value.priceAutoRefresh === 'on_startup'}
          onCheckedChange={(v) =>
            void handleBookSetting('priceAutoRefresh', v ? 'on_startup' : 'off')}
          disabled={!dbSession.value || savingBookKey}
          aria-label="Auto-refresh prices on portfolio open"
        />
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Reminder lookahead</span>
          <span class="text-xs text-muted-foreground">
            How far ahead scheduled transactions surface as reminders
          </span>
        </div>
        {#if dbSession.value}
          <Select.Root
            type="single"
            value={String(bookSettings.value.remindersLookaheadDays)}
            onValueChange={(v) => {
              const days = Number(v);
              if (Number.isInteger(days) && days >= 1 && days <= 90)
                void handleBookSetting('remindersLookaheadDays', days);
            }}
            items={lookaheadOptions}
            disabled={savingBookKey}
          >
            <Select.Trigger class="w-44">
              <SelectPrimitive.Value placeholder="Select window" />
            </Select.Trigger>
            <Select.Content>
              {#each lookaheadOptions as o (o.value)}
                <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        {:else}
          <span class="text-xs text-muted-foreground">No book open.</span>
        {/if}
      </div>
      {#if !dbSession.value}
        <p class="text-xs text-muted-foreground">
          Open or create a book to change reporting defaults — they travel with the file.
        </p>
      {/if}
    </Card.Content>
  </Card.Root>

  <!-- Data export -->
  <DataExport />

  <!-- Danger zone -->
  <Card.Root class="border-destructive/30">
    <IconCardHeader title="Danger zone" description="Destructive actions. These cannot be undone.">
      {#snippet icon()}
        <Trash2 aria-hidden="true" class="size-4 text-destructive" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col items-start gap-4">
      <div class="flex w-full items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Clear all data</span>
          <span class="text-xs text-muted-foreground">
            Delete accounts, transactions, scheduled transactions, budgets, prices and history from
            this book
          </span>
        </div>
        <Dialog.Root bind:open={clearDialogOpen}>
          <Button
            variant="destructive"
            disabled={!dbSession.value || clearing}
            onclick={() => (clearDialogOpen = true)}
          >
            Clear all data
          </Button>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>Clear all data?</Dialog.Title>
              <Dialog.Description>
                This will permanently delete all accounts, transactions, scheduled transactions,
                budgets, prices, symbol overrides and changes history. Your appearance preferences
                will be preserved.
              </Dialog.Description>
            </Dialog.Header>
            <Dialog.Footer>
              <Button
                variant="outline"
                onclick={() => (clearDialogOpen = false)}
                disabled={clearing}
              >
                Cancel
              </Button>
              <Button variant="destructive" onclick={handleClearDb} disabled={clearing}>
                {clearing ? 'Clearing...' : 'Yes, clear everything'}
              </Button>
            </Dialog.Footer>
          </Dialog.Content>
        </Dialog.Root>
      </div>
      {#if !dbSession.value}
        <p class="text-xs text-muted-foreground">No data imported yet — nothing to clear.</p>
      {/if}
      <div class="flex w-full items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Reset preferences</span>
          <span class="text-xs text-muted-foreground">
            Restore theme, density, locale and list settings to defaults
          </span>
        </div>
        <Dialog.Root bind:open={resetDialogOpen}>
          <Button variant="outline" onclick={() => (resetDialogOpen = true)}>
            <RotateCcw aria-hidden="true" class="size-4" />
            Reset to defaults
          </Button>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>Reset preferences?</Dialog.Title>
              <Dialog.Description>
                This restores all appearance and display settings to their defaults. Your book data
                is not touched.
              </Dialog.Description>
            </Dialog.Header>
            <Dialog.Footer>
              <Button variant="outline" onclick={() => (resetDialogOpen = false)}>Cancel</Button>
              <Button variant="destructive" onclick={handleReset}>Reset preferences</Button>
            </Dialog.Footer>
          </Dialog.Content>
        </Dialog.Root>
      </div>
    </Card.Content>
  </Card.Root>
</div>
