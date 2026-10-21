<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import { Select as SelectPrimitive } from 'bits-ui';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { CalendarDays } from '@lucide/svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { bookSettings, saveBookSetting } from '$lib/stores/book-settings.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import type { BookSettings, PnlPeriodId } from '$lib/tauri';
  import { monthName } from '$lib/utils/date-utils.svelte';

  const pnlPeriods: { value: PnlPeriodId; label: string }[] = [
    { value: 'this_month', label: 'This month' },
    { value: 'this_quarter', label: 'This quarter' },
    { value: 'this_year', label: 'This year (fiscal)' },
    { value: 'last_12_months', label: 'Last 12 months' }
  ];

  let savingBookKey = $state(false);

  const fyMonths = $derived(
    Array.from({ length: 12 }, (_, i) => ({
      value: String(i + 1),
      label: monthName(i + 1, appSettings.value.locale)
    }))
  );

  async function handleBookSetting<
    K extends 'fiscalYearStartMonth' | 'defaultPnlPeriod' | 'priceAutoRefresh'
  >(field: K, value: BookSettings[K]) {
    if (!dbSession.value || savingBookKey) return;
    savingBookKey = true;
    try {
      await saveBookSetting(field, value);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t save that setting'));
    } finally {
      savingBookKey = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Reporting"
    description="These live inside your .spendo file, so they follow the book around."
  >
    {#snippet icon()}
      <CalendarDays aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow
      label="Fiscal year starts"
      description="Sets what This-year means in P&amp;L; more grouping options are next"
      labelFor="setting-fy-start"
    >
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
          <Select.Trigger id="setting-fy-start" class="w-36">
            <SelectPrimitive.Value placeholder="Select month" />
          </Select.Trigger>
          <Select.Content>
            {#each fyMonths as m (m.value)}
              <Select.Item value={m.value} label={m.label}>{m.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      {:else}
        <span class="text-xs text-muted-foreground">Open a book to change this.</span>
      {/if}
    </SettingSelectRow>
    <SettingSelectRow
      label="Default P&amp;L period"
      description="Where Profit &amp; loss starts when the link has no dates"
      labelFor="setting-pnl-period"
    >
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
          <Select.Trigger id="setting-pnl-period" class="w-44">
            <SelectPrimitive.Value placeholder="Select period" />
          </Select.Trigger>
          <Select.Content>
            {#each pnlPeriods as p (p.value)}
              <Select.Item value={p.value} label={p.label}>{p.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      {:else}
        <span class="text-xs text-muted-foreground">Open a book to change this.</span>
      {/if}
    </SettingSelectRow>
    <SettingSelectRow
      label="Auto-refresh prices"
      description="Grab fresh Yahoo prices whenever you open Portfolio"
    >
      <Switch
        checked={bookSettings.value.priceAutoRefresh === 'on_startup'}
        onCheckedChange={(v) =>
          void handleBookSetting('priceAutoRefresh', v ? 'on_startup' : 'off')}
        disabled={!dbSession.value || savingBookKey}
        aria-label="Auto-refresh prices on portfolio open"
      />
    </SettingSelectRow>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">
        Open or create a book to change reporting defaults. They travel with the file.
      </p>
    {/if}
  </Card.Content>
</Card.Root>
