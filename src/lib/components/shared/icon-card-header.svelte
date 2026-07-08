<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import type { Snippet } from 'svelte';

  let {
    icon,
    title,
    description,
    class: className = ''
  }: {
    icon: Snippet;
    title: string;
    description?: string;
    class?: string;
  } = $props();
</script>

<Card.Header class={className}>
  <div class="flex items-center gap-2">
    {@render icon()}
    <Card.Title>{title}</Card.Title>
  </div>
  {#if description}
    <Card.Description>{description}</Card.Description>
  {/if}
</Card.Header>
