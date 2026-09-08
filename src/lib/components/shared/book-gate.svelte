<script lang="ts">
  import type { Snippet } from 'svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';

  let {
    title,
    description,
    children
  }: {
    title: string;
    description?: string;
    children: Snippet;
  } = $props();
</script>

{#if dbSession.value || dbBootstrapping.value}
  {@render children()}
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader {title} description={description ?? ''} />
    <ImportPrompt />
  </div>
{/if}
