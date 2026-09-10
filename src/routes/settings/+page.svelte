<script lang="ts">
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import AppearanceCard from '$lib/features/settings/appearance-card.svelte';
  import BookCard from '$lib/features/settings/book-card.svelte';
  import DangerZoneCard from '$lib/features/settings/danger-zone-card.svelte';
  import DataExport from '$lib/features/settings/data-export.svelte';
  import DataImport from '$lib/features/settings/data-import.svelte';
  import DisplayCard from '$lib/features/settings/display-card.svelte';
  import ReportingCard from '$lib/features/settings/reporting-card.svelte';
  import { appSettings, resetSettings } from '$lib/stores/settings.svelte';
  import { loadBookSettings } from '$lib/stores/book-settings.svelte';
  import { fetchDataInfo } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  const DEFAULT_CUSTOM_PRIMARY = '#3b82f6';

  let customDraft = $state(appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY);
  let bookSettingsError = $state<string | null>(null);

  $effect(() => {
    loadBookSettings().catch((e: unknown) => {
      bookSettingsError = errMsg(e, 'Failed to load book settings');
    });
  });

  /** Reload book settings after a switch/save and clear a stale error card. */
  async function refreshBookSettings() {
    bookSettingsError = null;
    try {
      await loadBookSettings(true);
    } catch (e: unknown) {
      bookSettingsError = errMsg(e, 'Failed to load book settings');
    }
  }

  const dataRes = useAsyncData({
    enabled: () => true,
    fetch: fetchDataInfo,
    label: 'Failed to load data location'
  });

  function handleResetPreferences() {
    resetSettings();
    customDraft = DEFAULT_CUSTOM_PRIMARY;
    toast.success('Preferences reset to defaults');
  }
</script>

<svelte:head>
  <title>Settings — Spendo</title>
  <meta name="description" content="Configure Spendo preferences" />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader title="Settings" description="Configure your preferences" />

  <AppearanceCard bind:customDraft />

  <DisplayCard />

  <BookCard
    dbPath={dataRes.data?.dbPath ?? null}
    dataError={dataRes.error}
    onRetryData={() => dataRes.reload()}
    onBookChanged={refreshBookSettings}
  />

  <ReportingCard error={bookSettingsError} onRetry={() => void refreshBookSettings()} />

  <DataImport onImported={() => dataRes.reload()} />

  <DataExport />

  <DangerZoneCard onResetPreferences={handleResetPreferences} />
</div>
