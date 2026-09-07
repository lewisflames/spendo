<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import { Select as SelectPrimitive } from 'bits-ui';
  import {
    appSettings,
    saveSettings,
    resetSettings,
    PAGE_SIZE_OPTIONS,
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
  import {
    SunMoon,
    Languages,
    ListOrdered,
    HardDrive,
    FolderOpen,
    Plus,
    Save,
    RotateCcw,
    Trash2
  } from '@lucide/svelte';
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

  let clearDialogOpen = $state(false);
  let clearing = $state(false);
  let savingCurrency = $state(false);
  let switching = $state(false);

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

  async function handleCurrencyChange(v: string | undefined) {
    if (!v || !dbSession.value || v === dbSession.value.summary.currency) return;
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

  function handleReset() {
    resetSettings();
    toast.success('Preferences reset to defaults');
  }
</script>

<svelte:head>
  <title>Settings — Spendo</title>
  <meta name="description" content="Configure Spendo preferences" />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader title="Settings" description="Configure your preferences">
    <Button variant="outline" onclick={handleReset}>
      <RotateCcw aria-hidden="true" class="size-4" />
      Reset to defaults
    </Button>
  </PageHeader>

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
          <span class="text-sm font-medium">Reduce motion</span>
          <span class="text-xs text-muted-foreground">Disable animated page transitions</span>
        </div>
        <Switch
          checked={appSettings.value.reduceMotion}
          onCheckedChange={(v) => saveSettings({ reduceMotion: v })}
          aria-label="Reduce motion"
        />
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <IconCardHeader title="Regional">
      {#snippet icon()}
        <Languages aria-hidden="true" class="size-4 text-muted-foreground" />
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
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <IconCardHeader title="Transactions">
      {#snippet icon()}
        <ListOrdered aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Page size</span>
          <span class="text-xs text-muted-foreground">Transactions per page</span>
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

  <!-- Data location -->
  <Card.Root>
    <IconCardHeader title="Data" description="Your book is a portable .spendo file.">
      {#snippet icon()}
        <HardDrive aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Base currency</span>
          <span class="text-xs text-muted-foreground">Report currency for the book</span>
        </div>
        {#if dbSession.value}
          <Select.Root
            type="single"
            value={dbSession.value.summary.currency}
            onValueChange={handleCurrencyChange}
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
          <span class="text-xs text-muted-foreground">No book yet — start from Overview.</span>
        {/if}
      </div>
      <div class="flex flex-col items-start gap-3">
        {#if dataRes.error}
          <ErrorCard message={dataRes.error} onRetry={() => dataRes.reload()} />
        {:else}
          <code
            class="max-w-full truncate rounded bg-muted px-2 py-1 font-mono text-xs text-muted-foreground"
            title={activePath ?? undefined}
          >
            {activePath ?? 'No book open'}
          </code>
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
              <FolderOpen aria-hidden="true" class="size-4" />
              Reveal in file manager
            </Button>
          </div>
        {/if}
      </div>
    </Card.Content>
  </Card.Root>

  <!-- Data export -->
  <DataExport />

  <!-- Clear database -->
  <Card.Root class="border-destructive/30">
    <IconCardHeader
      title="Clear database"
      description="Permanently delete all imported data. This action cannot be undone."
    >
      {#snippet icon()}
        <Trash2 aria-hidden="true" class="size-4 text-destructive" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col items-start gap-1.5">
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
              This will permanently delete all accounts, transactions, prices, and changes history.
              Your settings will be preserved.
            </Dialog.Description>
          </Dialog.Header>
          <Dialog.Footer>
            <Button variant="outline" onclick={() => (clearDialogOpen = false)} disabled={clearing}>
              Cancel
            </Button>
            <Button variant="destructive" onclick={handleClearDb} disabled={clearing}>
              {clearing ? 'Clearing...' : 'Yes, clear everything'}
            </Button>
          </Dialog.Footer>
        </Dialog.Content>
      </Dialog.Root>
      {#if !dbSession.value}
        <p class="text-xs text-muted-foreground">No data imported yet — nothing to clear.</p>
      {/if}
    </Card.Content>
  </Card.Root>
</div>
