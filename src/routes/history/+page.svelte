<script lang="ts">
  import ChangeHistory from '$lib/features/history/change-history.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
</script>

<svelte:head>
  <title>History — Spendo</title>
  <meta name="description" content="Spendo — imports, edits and reverts." />
</svelte:head>

<BookGate title="History" description="Imports, edits and reverts">
  <div class="flex flex-col gap-6">
    <PageHeader title="History" description="Imports, edits and reverts" />
    <ChangeHistory />
  </div>
</BookGate>
