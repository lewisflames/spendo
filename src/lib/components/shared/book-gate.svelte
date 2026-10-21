<script lang="ts">
  import type { Snippet } from 'svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import EmptyBookPrompt from '$lib/components/shared/empty-book-prompt.svelte';

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
  <div class="page-stack">
    <PageHeader {title} {description} />
    <EmptyBookPrompt />
  </div>
{/if}
