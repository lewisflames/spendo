<script lang="ts">
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import AppearanceCard from '$lib/features/settings/appearance-card.svelte';
  import BookCard from '$lib/features/settings/book-card.svelte';
  import DangerZoneCard from '$lib/features/settings/danger-zone-card.svelte';
  import ImportExportCard from '$lib/features/settings/import-export-card.svelte';
  import ImportRulesCard from '$lib/features/settings/import-rules-card.svelte';
  import DisplayCard from '$lib/features/settings/display-card.svelte';
  import ReportingCard from '$lib/features/settings/reporting-card.svelte';
  import { appSettings, resetSettings } from '$lib/stores/settings.svelte';
  import { loadBookSettings } from '$lib/stores/book-settings.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  const DEFAULT_CUSTOM_PRIMARY = '#3b82f6';

  let customDraft = $state(appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY);
  let bookSettingsError = $state<string | null>(null);

  $effect(() => {
    loadBookSettings().catch((e: unknown) => {
      bookSettingsError = errMsg(e, 'We couldn’t load your book settings');
    });
  });

  /** Reload book settings after a switch/save and clear a stale error card. */
  async function refreshBookSettings() {
    bookSettingsError = null;
    try {
      await loadBookSettings(true);
    } catch (e: unknown) {
      bookSettingsError = errMsg(e, 'We couldn’t load your book settings');
    }
  }

  function handleResetPreferences() {
    resetSettings();
    customDraft = DEFAULT_CUSTOM_PRIMARY;
    toast.success('Back to defaults. Fresh start');
  }
</script>

<svelte:head>
  <title>Settings · Spendo</title>
  <meta name="description" content="Configure Spendo preferences" />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader title="Settings" description="Make Spendo yours" />

  {#if bookSettingsError}
    <ErrorCard message={bookSettingsError} onRetry={() => void refreshBookSettings()} />
  {/if}

  <AppearanceCard bind:customDraft />

  <DisplayCard />

  <BookCard />

  <ReportingCard />

  <ImportExportCard />

  <ImportRulesCard />

  <DangerZoneCard onResetPreferences={handleResetPreferences} />
</div>
