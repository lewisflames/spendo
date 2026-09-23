<script lang="ts">
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { Button } from '$lib/components/ui/button';
  import { SearchX } from '@lucide/svelte';

  /**
   * Standard filtered-empty state: SearchX icon, "No matching …" title,
   * friendly retry hint and an outline Clear action.
   * Use for search-filtered tables; unfiltered empties keep bespoke copy.
   */
  let {
    title,
    description = 'Try a different search or clear the filters below.',
    onClear
  }: {
    title: string;
    description?: string;
    onClear: () => void;
  } = $props();
</script>

<EmptyState card {title} {description}>
  {#snippet icon()}
    <SearchX aria-hidden="true" class="text-muted-foreground" />
  {/snippet}
  {#snippet action()}
    <Button variant="outline" onclick={onClear}>Clear filters</Button>
  {/snippet}
</EmptyState>
