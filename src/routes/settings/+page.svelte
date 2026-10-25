<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import AppearanceCard from '$lib/features/settings/appearance-card.svelte';
  import BackupsCard from '$lib/features/settings/backups-card.svelte';
  import BookCard from '$lib/features/settings/book-card.svelte';
  import DangerZoneCard from '$lib/features/settings/danger-zone-card.svelte';
  import ImportExportCard from '$lib/features/settings/import-export-card.svelte';
  import ImportRulesCard from '$lib/features/settings/import-rules-card.svelte';
  import TagsCard from '$lib/features/settings/tags-card.svelte';
  import DisplayCard from '$lib/features/settings/display-card.svelte';
  import ReportingCard from '$lib/features/settings/reporting-card.svelte';
  import { appSettings, resetSettings } from '$lib/stores/settings.svelte';
  import { loadBookSettings } from '$lib/stores/book-settings.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  const meta = PAGE_META.settings;

  /** In-page jump links; ids match the section wrappers below. */
  const SECTIONS = [
    { id: 'appearance', label: 'Appearance' },
    { id: 'display', label: 'Display & lists' },
    { id: 'book', label: 'Book' },
    { id: 'backups', label: 'Backups' },
    { id: 'reporting', label: 'Reporting' },
    { id: 'import-export', label: 'Import & export' },
    { id: 'import-rules', label: 'Import rules' },
    { id: 'tags', label: 'Tags' },
    { id: 'danger-zone', label: 'Danger zone' }
  ];

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
    toast.success('Preferences reset');
  }
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<div class="page-stack">
  <PageHeader title={meta.title} description={meta.description} />

  {#if bookSettingsError}
    <ErrorCard message={bookSettingsError} onRetry={() => void refreshBookSettings()} />
  {/if}

  <nav aria-label="Settings sections" class="flex flex-wrap gap-1.5">
    {#each SECTIONS as section (section.id)}
      <a
        href="#{section.id}"
        class="rounded-full border px-3 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >{section.label}</a
      >
    {/each}
  </nav>

  <section id="appearance" class="scroll-mt-4"><AppearanceCard bind:customDraft /></section>
  <section id="display" class="scroll-mt-4"><DisplayCard /></section>
  <section id="book" class="scroll-mt-4"><BookCard /></section>
  <section id="backups" class="scroll-mt-4"><BackupsCard /></section>
  <section id="reporting" class="scroll-mt-4"><ReportingCard /></section>
  <section id="import-export" class="scroll-mt-4"><ImportExportCard /></section>
  <section id="import-rules" class="scroll-mt-4"><ImportRulesCard /></section>
  <section id="tags" class="scroll-mt-4"><TagsCard /></section>
  <section id="danger-zone" class="scroll-mt-4">
    <DangerZoneCard onResetPreferences={handleResetPreferences} />
  </section>
</div>
