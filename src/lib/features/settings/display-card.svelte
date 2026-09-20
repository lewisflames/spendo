<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import {
    appSettings,
    saveSettings,
    PAGE_SIZE_OPTIONS,
    type Locale
  } from '$lib/stores/settings.svelte';
  import { ListOrdered } from '@lucide/svelte';
  import SettingSelectRow from './setting-select-row.svelte';

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
</script>

<Card.Root>
  <IconCardHeader title="Display & lists">
    {#snippet icon()}
      <ListOrdered aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow label="Locale" description="Date, number and currency display format">
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
    </SettingSelectRow>
    <SettingSelectRow
      label="Rows per page"
      description="Applies to every list: transactions, ledger, holdings"
    >
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
            <Select.Item value={String(opt)} label={`${opt} per page`}>{opt} per page</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </SettingSelectRow>
  </Card.Content>
</Card.Root>
