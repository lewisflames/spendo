<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { ArrowLeftRight } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { bookSettings, saveBookSetting } from '$lib/stores/book-settings.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import type { CsvDelimiter, ExportDateFormat } from '$lib/tauri';

  let { error, onRetry }: { error: string | null; onRetry: () => void } = $props();

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
      toast.error(errMsg(e, 'Failed to save export preference'));
    } finally {
      savingPref = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Import &amp; export"
    description="CSV formatting for exports. Import and export run from the book menu or command palette — CSV exports are for interchange; use Save as… for a full backup."
  >
    {#snippet icon()}
      <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    {#if error}
      <ErrorCard message={error} {onRetry} />
    {/if}
    <SettingSelectRow label="CSV delimiter" description="Use semicolon for Excel in de-DE / fr-FR">
      <Select.Root
        type="single"
        value={bookSettings.value.exportCsvDelimiter}
        onValueChange={(v) => {
          if (v === ',' || v === ';') void handleExportPref('exportCsvDelimiter', v);
        }}
        items={delimiters}
        disabled={!dbSession.value || savingPref}
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
    </SettingSelectRow>
    <SettingSelectRow
      label="CSV date format"
      description="ISO stays machine-readable for re-import"
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
        <Select.Trigger class="w-44">
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
      <p class="text-xs text-muted-foreground">
        Open or create a book to change export formatting — it travels with the file.
      </p>
    {/if}
  </Card.Content>
</Card.Root>
