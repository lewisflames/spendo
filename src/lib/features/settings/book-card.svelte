<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { BOOK_CURRENCIES, defaultCurrencyForLocale } from '$lib/utils/currency';
  import { ensureSpendoExtension } from '$lib/utils/book';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { resolve } from '$app/paths';
  import { HardDrive, FolderOpen, Save, Plus, ExternalLink, Copy, Check } from '@lucide/svelte';
  import {
    dbSession,
    activeBookPath,
    setActiveBookPath,
    setDbSession
  } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { createBookAt, openBook, saveBookAs, setBaseCurrency } from '$lib/tauri';

  let {
    dbPath,
    dataError,
    onRetryData,
    onBookChanged
  }: {
    dbPath: string | null;
    dataError: string | null;
    onRetryData: () => void;
    onBookChanged: () => Promise<void>;
  } = $props();

  let switching = $state(false);
  let copied = $state(false);
  let savingCurrency = $state(false);
  let currencyDialogOpen = $state(false);
  let pendingCurrency = $state<string | null>(null);

  const activePath = $derived(activeBookPath.value ?? dbPath);

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
      await onBookChanged();
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
      await onBookChanged();
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
      await onBookChanged();
      toast.success(`Book saved as ${await basename(file)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to save book'));
    } finally {
      switching = false;
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
</script>

<Card.Root>
  <IconCardHeader title="Book" description="Your book is a portable .spendo file.">
    {#snippet icon()}
      <HardDrive aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow
      label="Base currency"
      description="Report currency for the book — revalues net worth, reports and portfolio"
    >
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
    </SettingSelectRow>
    <div class="flex flex-col items-start gap-3">
      {#if dataError}
        <ErrorCard message={dataError} onRetry={onRetryData} />
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
          <Button variant="outline" disabled={switching || !dbSession.value} onclick={handleSaveAs}>
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
