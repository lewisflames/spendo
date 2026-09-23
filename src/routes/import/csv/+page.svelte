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

<BookGate title="Import CSV" description="Map columns, dry-run the preview, then import.">
  <div class="mx-auto flex w-full max-w-5xl flex-col gap-6">
    <PageHeader title="Import CSV" description="Map columns, dry-run the preview, then import.">
      <Button variant="outline" href={resolve('/transactions')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Transactions
      </Button>
    </PageHeader>
    <CsvImporter />
  </div>
</BookGate>
