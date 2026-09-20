<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Download, FileDown, LoaderCircle } from '@lucide/svelte';
  import { save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { dbSession } from '$lib/stores/db.svelte';
  import {
    bookSettings,
    loadBookSettings,
    saveBookSetting
  } from '$lib/stores/book-settings.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { writeTextFile, type CsvDelimiter, type ExportDateFormat } from '$lib/tauri';
  import { ensureCsvExtension } from '$lib/utils/book';
  import { errMsg } from '$lib/utils/error';
  import {
    ACCOUNTS_EXPORT_FILENAME,
    TRANSACTIONS_EXPORT_FILENAME,
    buildAccountsExport,
    buildTransactionsExport
  } from '$lib/features/book/exporters';

  let exportingTransactions = $state(false);
  let exportingAccounts = $state(false);
  let savingExportPref = $state(false);

  let busy = $derived(exportingTransactions || exportingAccounts);

  const delimiters: { value: CsvDelimiter; label: string }[] = [
    { value: ',', label: 'Comma (,)' },
    { value: ';', label: 'Semicolon (;)' }
  ];

  const dateFormats: { value: ExportDateFormat; label: string }[] = [
    { value: 'iso', label: 'ISO (YYYY-MM-DD)' },
    { value: 'locale', label: 'Locale format' }
  ];

  $effect(() => {
    loadBookSettings().catch(() => {});
  });

  async function handleExportPref(
    field: 'exportCsvDelimiter' | 'exportDateFormat',
    value: CsvDelimiter | ExportDateFormat
  ) {
    if (!dbSession.value || savingExportPref) return;
    savingExportPref = true;
    try {
      await saveBookSetting(field, value);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to save export preference'));
    } finally {
      savingExportPref = false;
    }
  }

  async function handleExport(kind: 'transactions' | 'accounts') {
    const isTransactions = kind === 'transactions';
    if (busy) return;
    let target: string | null;
    try {
      target = await save({
        defaultPath: isTransactions ? TRANSACTIONS_EXPORT_FILENAME : ACCOUNTS_EXPORT_FILENAME,
        filters: [{ name: 'CSV', extensions: ['csv'] }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return;
    }
    // Null means the user cancelled — stay silent.
    if (!target) return;
    const file = ensureCsvExtension(target);
    if (isTransactions) exportingTransactions = true;
    else exportingAccounts = true;
    try {
      const opts = {
        delimiter: bookSettings.value.exportCsvDelimiter,
        dateFormat: bookSettings.value.exportDateFormat,
        locale: appSettings.value.locale
      };
      const contents = isTransactions
        ? await buildTransactionsExport(opts)
        : await buildAccountsExport(opts);
      await writeTextFile(file, contents);
      toast.success(`Saved ${await basename(file)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, `Failed to export ${kind}`));
    } finally {
      if (isTransactions) exportingTransactions = false;
      else exportingAccounts = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader title="Data export" description="Full-book CSV dumps saved via the file dialog.">
    {#snippet icon()}
      <FileDown aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">CSV delimiter</span>
        <span class="text-xs text-muted-foreground">Use semicolon for Excel in de-DE / fr-FR</span>
      </div>
      <Select.Root
        type="single"
        value={bookSettings.value.exportCsvDelimiter}
        onValueChange={(v) => {
          if (v === ',' || v === ';') void handleExportPref('exportCsvDelimiter', v);
        }}
        items={delimiters}
        disabled={!dbSession.value || savingExportPref}
      >
        <Select.Trigger class="w-44">
          <SelectPrimitive.Value placeholder="Select delimiter" />
        </Select.Trigger>
        <Select.Content>
          {#each delimiters as d (d.value)}
            <Select.Item value={d.value} label={d.label}>{d.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">CSV date format</span>
        <span class="text-xs text-muted-foreground">ISO stays machine-readable for re-import</span>
      </div>
      <Select.Root
        type="single"
        value={bookSettings.value.exportDateFormat}
        onValueChange={(v) => {
          if (v === 'iso' || v === 'locale') void handleExportPref('exportDateFormat', v);
        }}
        items={dateFormats}
        disabled={!dbSession.value || savingExportPref}
      >
        <Select.Trigger class="w-44">
          <SelectPrimitive.Value placeholder="Select format" />
        </Select.Trigger>
        <Select.Content>
          {#each dateFormats as f (f.value)}
            <Select.Item value={f.value} label={f.label}>{f.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">Transactions</span>
        <span class="text-xs text-muted-foreground"
          >One row per split, newest first — GnuCash multi-split compatible</span
        >
      </div>
      <Button
        variant="outline"
        disabled={!dbSession.value || busy}
        onclick={() => void handleExport('transactions')}
      >
        {#if exportingTransactions}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Exporting...
        {:else}
          <Download aria-hidden="true" class="size-4" />
          Export
        {/if}
      </Button>
    </div>
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">Accounts</span>
        <span class="text-xs text-muted-foreground"
          >Full account tree with hierarchy and commodities</span
        >
      </div>
      <Button
        variant="outline"
        disabled={!dbSession.value || busy}
        onclick={() => void handleExport('accounts')}
      >
        {#if exportingAccounts}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Exporting...
        {:else}
          <Download aria-hidden="true" class="size-4" />
          Export
        {/if}
      </Button>
    </div>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">No book yet — open or create one from Welcome.</p>
    {/if}
  </Card.Content>
</Card.Root>
