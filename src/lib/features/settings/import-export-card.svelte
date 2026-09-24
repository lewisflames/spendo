<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { ArrowLeftRight } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { bookSettings, saveBookSetting } from '$lib/stores/book-settings.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import type { CsvDelimiter, ExportDateFormat } from '$lib/tauri';

  let savingPref = $state(false);

  const delimiters: { value: CsvDelimiter; label: string }[] = [
    { value: ',', label: 'Comma (,)' },
    { value: ';', label: 'Semicolon (;)' }
  ];

  const dateFormats: { value: ExportDateFormat; label: string }[] = [
    { value: 'iso', label: 'ISO (YYYY-MM-DD)' },
    { value: 'locale', label: 'Locale format' }
  ];

  async function handleExportPref(
    field: 'exportCsvDelimiter' | 'exportDateFormat',
    value: CsvDelimiter | ExportDateFormat
  ) {
    if (!dbSession.value || savingPref) return;
    savingPref = true;
    try {
      await saveBookSetting(field, value);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t save that preference. Please try again'));
    } finally {
      savingPref = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Import &amp; export"
    description="How your CSVs look on the way out. Find import and export in the book menu or command palette. For a full backup, use Save as… instead."
  >
    {#snippet icon()}
      <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow
      label="CSV delimiter"
      description="Semicolon plays nicer with some Excels (de-DE / fr-FR)"
      labelFor="setting-csv-delimiter"
    >
      <Select.Root
        type="single"
        value={bookSettings.value.exportCsvDelimiter}
        onValueChange={(v) => {
          if (v === ',' || v === ';') void handleExportPref('exportCsvDelimiter', v);
        }}
        items={delimiters}
        disabled={!dbSession.value || savingPref}
      >
        <Select.Trigger id="setting-csv-delimiter" class="w-44">
          <SelectPrimitive.Value placeholder="Select delimiter" />
        </Select.Trigger>
        <Select.Content>
          {#each delimiters as d (d.value)}
            <Select.Item value={d.value} label={d.label}>{d.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </SettingSelectRow>
    <SettingSelectRow
      label="CSV date format"
      description="ISO is easiest if you ever bring the file back in"
      labelFor="setting-csv-date-format"
    >
      <Select.Root
        type="single"
        value={bookSettings.value.exportDateFormat}
        onValueChange={(v) => {
          if (v === 'iso' || v === 'locale') void handleExportPref('exportDateFormat', v);
        }}
        items={dateFormats}
        disabled={!dbSession.value || savingPref}
      >
        <Select.Trigger id="setting-csv-date-format" class="w-44">
          <SelectPrimitive.Value placeholder="Select format" />
        </Select.Trigger>
        <Select.Content>
          {#each dateFormats as f (f.value)}
            <Select.Item value={f.value} label={f.label}>{f.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </SettingSelectRow>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">Open a book to change this.</p>
    {/if}
  </Card.Content>
</Card.Root>
