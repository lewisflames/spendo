<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import CsvImporter from '$lib/features/csv-import/csv-importer.svelte';
  import { resolve } from '$app/paths';

  const meta = PAGE_META.importCsv;
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-form page-stack">
    <PageHeader
      title={meta.title}
      description={meta.description}
      breadcrumbs={[{ label: PAGE_META.transactions.title, href: resolve('/transactions') }]}
    />
    <CsvImporter />
  </div>
</BookGate>
