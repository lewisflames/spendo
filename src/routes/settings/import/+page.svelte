<script lang="ts">
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import CsvImporter from '$lib/features/csv-import/csv-importer.svelte';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import { ArrowLeft } from '@lucide/svelte';
</script>

<svelte:head>
  <title>Import CSV — Spendo</title>
  <meta name="description" content="Import bank-statement CSV with column mapping and preview." />
</svelte:head>

<BookGate title="Import CSV" description="Bank-statement CSV with mapping and preview">
  <div class="flex flex-col gap-6">
    <PageHeader title="Import CSV" description="Map columns, dry-run the preview, then import.">
      <Button variant="outline" href={resolve('/settings')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Settings
      </Button>
    </PageHeader>
    <CsvImporter />
  </div>
</BookGate>
